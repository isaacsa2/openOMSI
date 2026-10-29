
/// Binding types of the port's WGSL, reflected with naga.
struct Reflection {
    module: naga::Module,
    /// Textures that the entry point samples with a filtering sampler.
    filtered: Vec<naga::Handle<naga::GlobalVariable>>,
}

impl Reflection {
    fn new(source: &str) -> Result<Self, FfxErrorCode> {
        let module =
            naga::front::wgsl::parse_str(source).map_err(|_| FFX_ERROR_BACKEND_API_ERROR)?;
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .map_err(|_| FFX_ERROR_BACKEND_API_ERROR)?;
        let entry = module
            .entry_points
            .iter()
            .position(|e| e.name == FFX_WGSL_ENTRY_POINT)
            .ok_or(FFX_ERROR_BACKEND_API_ERROR)?;
        let filtered = info
            .get_entry_point(entry)
            .sampling_set
            .iter()
            .filter(|key| {
                !matches!(
                    module.types[module.global_variables[key.sampler].ty].inner,
                    naga::TypeInner::Sampler { comparison: true }
                )
            })
            .map(|key| key.image)
            .collect();
        Ok(Self { module, filtered })
    }

    /// The layout type, array count and (for a uniform) size of group 0's
    /// `binding`.
    fn binding(
        &self,
        binding: u32,
    ) -> Result<(wgpu::BindingType, Option<NonZeroU32>, Option<u64>), FfxErrorCode> {
        let (handle, global) = self
            .module
            .global_variables
            .iter()
            .find(|(_, g)| {
                g.binding
                    .as_ref()
                    .is_some_and(|b| b.group == 0 && b.binding == binding)
            })
            .ok_or(FFX_ERROR_BACKEND_API_ERROR)?;
        let mut ty = &self.module.types[global.ty].inner;
        let mut count = None;
        if let naga::TypeInner::BindingArray { base, size } = ty {
            let naga::ArraySize::Constant(size) = size else {
                return Err(FFX_ERROR_BACKEND_API_ERROR);
            };
            count = Some(*size);
            ty = &self.module.types[*base].inner;
        }
        let buffer = |ty| wgpu::BindingType::Buffer {
            ty,
            has_dynamic_offset: false,
            min_binding_size: None,
        };
        let binding_type = match (global.space, ty) {
            (naga::AddressSpace::Uniform, _) => {
                let size = u64::from(ty.size(self.module.to_ctx()));
                return Ok((buffer(wgpu::BufferBindingType::Uniform), count, Some(size)));
            }
            (naga::AddressSpace::Storage { access }, _) => {
                buffer(wgpu::BufferBindingType::Storage {
                    read_only: !access.contains(naga::StorageAccess::STORE),
                })
            }
            (_, naga::TypeInner::Sampler { comparison }) => {
                wgpu::BindingType::Sampler(if *comparison {
                    wgpu::SamplerBindingType::Comparison
                } else {
                    wgpu::SamplerBindingType::Filtering
                })
            }
            (
                _,
                naga::TypeInner::Image {
                    dim,
                    arrayed,
                    class,
                },
            ) => {
                let view_dimension = match (dim, arrayed) {
                    (naga::ImageDimension::D1, _) => wgpu::TextureViewDimension::D1,
                    (naga::ImageDimension::D2, false) => wgpu::TextureViewDimension::D2,
                    (naga::ImageDimension::D2, true) => wgpu::TextureViewDimension::D2Array,
                    (naga::ImageDimension::D3, _) => wgpu::TextureViewDimension::D3,
                    (naga::ImageDimension::Cube, false) => wgpu::TextureViewDimension::Cube,
                    (naga::ImageDimension::Cube, true) => wgpu::TextureViewDimension::CubeArray,
                };
                match class {
                    naga::ImageClass::Sampled { kind, multi } => wgpu::BindingType::Texture {
                        sample_type: match kind {
                            naga::ScalarKind::Uint => wgpu::TextureSampleType::Uint,
                            naga::ScalarKind::Sint => wgpu::TextureSampleType::Sint,
                            _ => wgpu::TextureSampleType::Float {
                                filterable: self.filtered.contains(&handle),
                            },
                        },
                        view_dimension,
                        multisampled: *multi,
                    },
                    naga::ImageClass::Depth { multi } => wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension,
                        multisampled: *multi,
                    },
                    naga::ImageClass::Storage { format, access } => {
                        let load = access.contains(naga::StorageAccess::LOAD);
                        let store = access.contains(naga::StorageAccess::STORE);
                        let atomic = access.contains(naga::StorageAccess::ATOMIC);
                        wgpu::BindingType::StorageTexture {
                            access: match (atomic, load, store) {
                                (true, _, _) => wgpu::StorageTextureAccess::Atomic,
                                (false, true, true) => wgpu::StorageTextureAccess::ReadWrite,
                                (false, false, true) => wgpu::StorageTextureAccess::WriteOnly,
                                _ => wgpu::StorageTextureAccess::ReadOnly,
                            },
                            format: wgpu_naga_bridge::map_storage_format_from_naga(*format),
                            view_dimension,
                        }
                    }
                    naga::ImageClass::External => return Err(FFX_ERROR_BACKEND_API_ERROR),
                }
            }
            _ => return Err(FFX_ERROR_BACKEND_API_ERROR),
        };
        Ok((binding_type, count, None))
    }
}

fn sampler(
    description: &FfxSamplerDescription,
) -> Result<wgpu::SamplerDescriptor<'static>, FfxErrorCode> {
    let address = |mode| match mode {
        FfxAddressMode::Wrap => Ok(wgpu::AddressMode::Repeat),
        FfxAddressMode::Mirror => Ok(wgpu::AddressMode::MirrorRepeat),
        FfxAddressMode::Clamp => Ok(wgpu::AddressMode::ClampToEdge),
        FfxAddressMode::Border => Ok(wgpu::AddressMode::ClampToBorder),
        FfxAddressMode::MirrorOnce => Err(FFX_ERROR_INVALID_ENUM),
    };
    let (filter, mipmap_filter) = match description.filter {
        FfxFilterType::MinMagMipPoint => {
            (wgpu::FilterMode::Nearest, wgpu::MipmapFilterMode::Nearest)
        }
        FfxFilterType::MinMagMipLinear => {
            (wgpu::FilterMode::Linear, wgpu::MipmapFilterMode::Linear)
        }
        FfxFilterType::MinMagLinearMipPoint => {
            (wgpu::FilterMode::Linear, wgpu::MipmapFilterMode::Nearest)
        }
    };
    Ok(wgpu::SamplerDescriptor {
        label: Some("FfxSamplerDescription"),
        address_mode_u: address(description.address_mode_u)?,
        address_mode_v: address(description.address_mode_v)?,
        address_mode_w: address(description.address_mode_w)?,
        mag_filter: filter,
        min_filter: filter,
        mipmap_filter,
        ..Default::default()
    })
}

/// The WGSL storage format of a clear target.
fn storage_format_name(format: wgpu::TextureFormat) -> Option<&'static str> {
    use wgpu::TextureFormat as T;
    Some(match format {
        T::Rgba32Float => "rgba32float",
        T::Rgba16Float => "rgba16float",
        T::Rg32Float => "rg32float",
        T::Rg16Float => "rg16float",
        T::R32Float => "r32float",
        T::R16Float => "r16float",
        T::Rg11b10Ufloat => "rg11b10ufloat",
        T::Rgb10a2Unorm => "rgb10a2unorm",
        T::Rgba8Unorm => "rgba8unorm",
        T::Rgba8Snorm => "rgba8snorm",
        T::Bgra8Unorm => "bgra8unorm",
        T::Rg8Unorm => "rg8unorm",
        T::R8Unorm => "r8unorm",
        T::R16Unorm => "r16unorm",
        T::R16Snorm => "r16snorm",
        _ => return None,
    })
}

/// The wgpu format of a texture the effect creates: an `R32G32_FLOAT` UAV is
/// `Rgba32Float`, whose `.xy` the port reads and writes, because Metal has no
/// read-write `Rg32Float` storage texture and FSR2 reads and writes
/// `FSR2_AutoExposure` (`ffx_fsr2_callbacks_hlsl.h:467`, `:898`, `:907`).
/// SRV readers of the texture use its `.xy` as well.
fn storage_texture_format(
    description: &FfxResourceDescription,
) -> Result<wgpu::TextureFormat, FfxErrorCode> {
    if description.format == FfxSurfaceFormat::R32G32Float
        && description.usage & FFX_RESOURCE_USAGE_UAV != 0
    {
        return Ok(wgpu::TextureFormat::Rgba32Float);
    }
    texture_format(description.format).ok_or(FFX_ERROR_INVALID_ENUM)
}

/// `FfxSurfaceFormat` to wgpu. The typeless formats and `Unknown` have no
/// wgpu equivalent.
fn texture_format(format: FfxSurfaceFormat) -> Option<wgpu::TextureFormat> {
    use FfxSurfaceFormat as F;
    use wgpu::TextureFormat as T;
    Some(match format {
        F::R32G32B32A32Uint => T::Rgba32Uint,
        F::R32G32B32A32Float => T::Rgba32Float,
        F::R16G16B16A16Float => T::Rgba16Float,
        F::R32G32Float => T::Rg32Float,
        F::R8Uint => T::R8Uint,
        F::R32Uint => T::R32Uint,
        F::R8G8B8A8Unorm => T::Rgba8Unorm,
        F::R8G8B8A8Snorm => T::Rgba8Snorm,
        F::R8G8B8A8Srgb => T::Rgba8UnormSrgb,
        F::B8G8R8A8Unorm => T::Bgra8Unorm,
        F::B8G8R8A8Srgb => T::Bgra8UnormSrgb,
        F::R11G11B10Float => T::Rg11b10Ufloat,
        F::R10G10B10A2Unorm => T::Rgb10a2Unorm,
        F::R16G16Float => T::Rg16Float,
        F::R16G16Uint => T::Rg16Uint,
        F::R16G16Sint => T::Rg16Sint,
        F::R16Float => T::R16Float,
        F::R16Uint => T::R16Uint,
        F::R16Unorm => T::R16Unorm,
        F::R16Snorm => T::R16Snorm,
        F::R8Unorm => T::R8Unorm,
        F::R8G8Unorm => T::Rg8Unorm,
        F::R8G8Uint => T::Rg8Uint,
        F::R32Float => T::R32Float,
        F::R9G9B9E5Sharedexp => T::Rgb9e5Ufloat,
        _ => return None,
    })
}

/// wgpu to `FfxSurfaceFormat`; `Unknown` where the SDK has no equivalent.
fn surface_format(format: wgpu::TextureFormat) -> FfxSurfaceFormat {
    use FfxSurfaceFormat as F;
    use wgpu::TextureFormat as T;
    match format {
        T::Rgba32Uint => F::R32G32B32A32Uint,
        T::Rgba32Float => F::R32G32B32A32Float,
        T::Rgba16Float => F::R16G16B16A16Float,
        T::Rg32Float => F::R32G32Float,
        T::R8Uint => F::R8Uint,
        T::R32Uint => F::R32Uint,
        T::Rgba8Unorm => F::R8G8B8A8Unorm,
        T::Rgba8Snorm => F::R8G8B8A8Snorm,
        T::Rgba8UnormSrgb => F::R8G8B8A8Srgb,
        T::Bgra8Unorm => F::B8G8R8A8Unorm,
        T::Bgra8UnormSrgb => F::B8G8R8A8Srgb,
        T::Rg11b10Ufloat => F::R11G11B10Float,
        T::Rgb10a2Unorm => F::R10G10B10A2Unorm,
        T::Rg16Float => F::R16G16Float,
        T::Rg16Uint => F::R16G16Uint,
        T::Rg16Sint => F::R16G16Sint,
        T::R16Float => F::R16Float,
        T::R16Uint => F::R16Uint,
        T::R16Unorm => F::R16Unorm,
        T::R16Snorm => F::R16Snorm,
        T::R8Unorm => F::R8Unorm,
        T::Rg8Unorm => F::R8G8Unorm,
        T::Rg8Uint => F::R8G8Uint,
        T::R32Float | T::Depth32Float => F::R32Float,
        T::Rgb9e5Ufloat => F::R9G9B9E5Sharedexp,
        _ => F::Unknown,
    }
}


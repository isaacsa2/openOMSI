
impl FfxInterface for FfxWgpuBackend {
    fn device(&self) -> FfxDevice {
        1
    }

    fn get_sdk_version(&mut self) -> FfxVersionNumber {
        ffx_sdk_make_version(
            FFX_SDK_VERSION_MAJOR,
            FFX_SDK_VERSION_MINOR,
            FFX_SDK_VERSION_PATCH,
        )
    }

    fn get_effect_gpu_memory_usage(
        &mut self,
        effect_context_id: u32,
    ) -> Result<FfxEffectMemoryUsage, FfxErrorCode> {
        let mut usage = FfxEffectMemoryUsage::default();
        for resource in self.resources.iter().flatten() {
            if resource.effect_context_id == effect_context_id && !resource.dynamic {
                usage.total_usage_in_bytes += resource.bytes;
                if resource.handle.description.flags & FFX_RESOURCE_FLAGS_ALIASABLE != 0 {
                    usage.aliasable_usage_in_bytes += resource.bytes;
                }
            }
        }
        Ok(usage)
    }

    fn create_backend_context(
        &mut self,
        _effect: FfxEffect,
        _bindless_config: Option<&FfxEffectBindlessConfig>,
    ) -> Result<u32, FfxErrorCode> {
        let index = if let Some(index) = self.contexts.iter().position(Option::is_none) {
            self.contexts[index] = Some(EffectContext::default());
            index
        } else {
            self.contexts.push(Some(EffectContext::default()));
            self.contexts.len() - 1
        };
        u32::try_from(index).map_err(|_| FFX_ERROR_OUT_OF_RANGE)
    }

    fn get_device_capabilities(&mut self) -> Result<FfxDeviceCapabilities, FfxErrorCode> {
        let info = self.device.adapter_info();
        Ok(FfxDeviceCapabilities {
            // SDK-P6: wgpu cannot force a subgroup size, so do not report the
            // SM6.6 with which the SDK selects wave64 permutations.
            maximum_supported_shader_model: FfxShaderModel::ShaderModel6_2,
            wave_lane_count_min: info.subgroup_min_size,
            wave_lane_count_max: info.subgroup_max_size,
            fp16_supported: self.fp16,
            ..Default::default()
        })
    }

    fn destroy_backend_context(&mut self, effect_context_id: u32) -> Result<(), FfxErrorCode> {
        let context = self
            .contexts
            .get_mut(effect_context_id as usize)
            .and_then(Option::take)
            .ok_or(FFX_ERROR_INVALID_ARGUMENT)?;
        for index in context.dynamic {
            self.resources[index] = None;
            self.free_dynamic.push(index);
        }
        // Resources the effect did not destroy.
        for slot in &mut self.resources {
            if slot
                .as_ref()
                .is_some_and(|r| r.effect_context_id == effect_context_id)
                && let Some(resource) = slot.take()
            {
                self.handles.remove(&resource.handle_id);
            }
        }
        Ok(())
    }

    fn create_resource(
        &mut self,
        create_resource_description: &FfxCreateResourceDescription,
        effect_context_id: u32,
    ) -> Result<FfxResourceInternal, FfxErrorCode> {
        let d = create_resource_description;
        let description = d.resource_description;
        let (object, bytes) = create_object(&self.device, d)?;
        let handle = Handle {
            object,
            description,
            name: d.name.to_owned(),
        };
        let handle_id = self.insert_handle(handle.clone());
        let resource = self.insert_static(Resource {
            handle,
            state: d.initial_state,
            effect_context_id,
            dynamic: false,
            handle_id,
            upload_row_pitch: None,
            bytes,
        });

        // Initial data: an upload resource at the next index (which
        // ffxSafeReleaseCopyResource releases) and a scheduled copy job.
        let init = d.init_data;
        let data = match init.r#type {
            FfxResourceInitDataType::Buffer => init.buffer[..init.size].to_vec(),
            FfxResourceInitDataType::Value => vec![init.value; init.size],
            FfxResourceInitDataType::Invalid | FfxResourceInitDataType::Uninitialized => {
                return Ok(resource);
            }
        };
        let (contents, row_pitch) = match &self.resource(resource)?.handle.object {
            Object::Buffer(buffer) => {
                let mut contents = data;
                contents.resize(usize::try_from(buffer.size()).unwrap(), 0);
                (contents, None)
            }
            Object::Texture(texture) => {
                // An R32G32_FLOAT UAV's texels gain zero .zw (`storage_texture_format`).
                let data = if texture.format() == wgpu::TextureFormat::Rgba32Float
                    && description.format == FfxSurfaceFormat::R32G32Float
                {
                    data.chunks(8)
                        .flat_map(|texel| texel.iter().copied().chain([0; 8]))
                        .collect()
                } else {
                    data
                };
                let block = texture.format().block_copy_size(None).unwrap_or(0);
                let row = (texture.width() * block) as usize;
                let pitch =
                    (texture.width() * block).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
                let rows = (texture.height() * texture.depth_or_array_layers()) as usize;
                let mut contents = vec![0; pitch as usize * rows];
                for (y, source) in data.chunks(row).take(rows).enumerate() {
                    let start = y * pitch as usize;
                    contents[start..start + source.len()].copy_from_slice(source);
                }
                (contents, Some(pitch))
            }
        };
        let name = format!("{} upload", d.name);
        let upload = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&name),
                contents: &contents,
                usage: wgpu::BufferUsages::COPY_SRC,
            });
        let upload = Handle {
            object: Object::Buffer(upload),
            description,
            name,
        };
        let upload_handle_id = self.insert_handle(upload.clone());
        let src = self.insert_static(Resource {
            handle: upload,
            state: FFX_RESOURCE_STATE_COPY_SRC,
            effect_context_id,
            dynamic: false,
            handle_id: upload_handle_id,
            upload_row_pitch: row_pitch,
            bytes: contents.len() as u64,
        });
        self.schedule_gpu_job(&FfxGpuJobDescription {
            job_label: "Resource initialization".into(),
            descriptor: FfxGpuJobDescriptor::Copy(FfxCopyJobDescription {
                src,
                src_offset: 0,
                dst: resource,
                dst_offset: 0,
                size: 0,
            }),
        })?;
        Ok(resource)
    }

    fn register_resource(
        &mut self,
        in_resource: &FfxResource,
        effect_context_id: u32,
    ) -> Result<FfxResourceInternal, FfxErrorCode> {
        if in_resource.resource == 0 {
            return Ok(FfxResourceInternal { internal_index: 0 });
        }
        let handle = self
            .handles
            .get(&in_resource.resource)
            .ok_or(FFX_ERROR_INVALID_ARGUMENT)?;
        let resource = Resource {
            handle: Handle {
                object: handle.object.clone(),
                description: in_resource.description,
                name: if in_resource.name.is_empty() {
                    handle.name.clone()
                } else {
                    in_resource.name.clone()
                },
            },
            state: in_resource.state,
            effect_context_id,
            dynamic: true,
            handle_id: in_resource.resource,
            upload_row_pitch: None,
            bytes: 0,
        };
        let context = self
            .contexts
            .get_mut(effect_context_id as usize)
            .and_then(Option::as_mut)
            .ok_or(FFX_ERROR_INVALID_ARGUMENT)?;
        let index = if let Some(index) = self.free_dynamic.pop() {
            self.resources[index] = Some(resource);
            index
        } else {
            self.resources.push(Some(resource));
            self.resources.len() - 1
        };
        context.dynamic.push(index);
        Ok(FfxResourceInternal {
            internal_index: i32::try_from(index).map_err(|_| FFX_ERROR_OUT_OF_RANGE)?,
        })
    }

    fn get_resource(&mut self, resource: FfxResourceInternal) -> FfxResource {
        match self.resource(resource) {
            Ok(r) => FfxResource {
                resource: r.handle_id,
                description: r.handle.description,
                state: r.state,
                name: r.handle.name.clone(),
            },
            Err(_) => FfxResource::default(),
        }
    }

    fn unregister_resources(
        &mut self,
        _command_list: FfxCommandList,
        effect_context_id: u32,
    ) -> Result<(), FfxErrorCode> {
        let context = self
            .contexts
            .get_mut(effect_context_id as usize)
            .and_then(Option::as_mut)
            .ok_or(FFX_ERROR_INVALID_ARGUMENT)?;
        for index in context.dynamic.drain(..) {
            self.resources[index] = None;
            self.free_dynamic.push(index);
        }
        Ok(())
    }

    fn get_resource_description(
        &mut self,
        resource: FfxResourceInternal,
    ) -> FfxResourceDescription {
        self.resource(resource)
            .map(|r| r.handle.description)
            .unwrap_or_default()
    }

    fn destroy_resource(
        &mut self,
        resource: FfxResourceInternal,
        _effect_context_id: u32,
    ) -> Result<(), FfxErrorCode> {
        if let Some(slot) = usize::try_from(resource.internal_index)
            .ok()
            .and_then(|index| self.resources.get_mut(index))
            && slot.as_ref().is_some_and(|r| !r.dynamic)
            && let Some(r) = slot.take()
        {
            self.handles.remove(&r.handle_id);
        }
        Ok(())
    }

    fn stage_constant_buffer_data_func(
        &mut self,
        data: &[u32],
        constant_buffer: &mut FfxConstantBuffer,
    ) -> Result<(), FfxErrorCode> {
        constant_buffer.data = data.to_vec();
        constant_buffer.num_32bit_entries =
            u32::try_from(data.len()).map_err(|_| FFX_ERROR_INVALID_SIZE)?;
        Ok(())
    }

    fn create_pipeline(
        &mut self,
        effect: FfxEffect,
        pass: FfxPass,
        permutation_options: u32,
        pipeline_description: &FfxPipelineDescription<'_>,
        _effect_context_id: u32,
    ) -> Result<FfxPipelineState, FfxErrorCode> {
        let blob = self.get_permutation_blob_by_index(
            effect,
            pass,
            pipeline_description.stage,
            permutation_options,
        )?;
        if blob.permutation.wave64 {
            // SDK-P6: not selected, because the backend reports SM6.2.
            return Err(FFX_ERROR_BACKEND_API_ERROR);
        }
        let source = ffx_get_wgsl_source(&blob).ok_or(FFX_ERROR_BACKEND_API_ERROR)?;
        let reflection = Reflection::new(&source)?;

        let tables: [(&[FfxShaderBlobBinding], u32); 6] = [
            (blob.bound_constant_buffers, FFX_WGSL_BINDING_OFFSET_CBV),
            (blob.bound_srv_textures, FFX_WGSL_BINDING_OFFSET_SRV),
            (blob.bound_uav_textures, FFX_WGSL_BINDING_OFFSET_UAV),
            (blob.bound_srv_buffers, FFX_WGSL_BINDING_OFFSET_SRV),
            (blob.bound_uav_buffers, FFX_WGSL_BINDING_OFFSET_UAV),
            (blob.bound_samplers, FFX_WGSL_BINDING_OFFSET_SAMPLER),
        ];
        let mut entries = HashMap::new();
        let mut constant_sizes = HashMap::new();
        for (table, offset) in tables {
            for b in table {
                let binding = offset + b.binding;
                let (ty, count, size) = reflection.binding(binding)?;
                if count.map_or(1, NonZeroU32::get) != b.count {
                    return Err(FFX_ERROR_BACKEND_API_ERROR);
                }
                if let Some(size) = size {
                    constant_sizes.insert(binding, size);
                }
                entries.insert(
                    binding,
                    wgpu::BindGroupLayoutEntry {
                        binding,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty,
                        count,
                    },
                );
            }
        }
        let label = Some(pipeline_description.name.as_str());
        let mut layout_entries: Vec<_> = entries.values().copied().collect();
        layout_entries.sort_by_key(|e| e.binding);
        let layout = self
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label,
                entries: &layout_entries,
            });
        let indirect = pipeline_description.indirect_workload != 0;
        let mut groups = vec![Some(&layout)];
        if indirect {
            groups.push(Some(&self.grid.layout));
        }
        let pipeline_layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label,
                bind_group_layouts: &groups,
                immediate_size: 0,
            });
        let module = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label,
                source: wgpu::ShaderSource::Wgsl(
                    if indirect {
                        dispatch_grid::wgsl_entry(&source)
                    } else {
                        source
                    }
                    .into(),
                ),
            });
        let pipeline = self
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label,
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some(FFX_WGSL_ENTRY_POINT),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            });
        // Static samplers: register sN is the description's Nth sampler.
        let samplers = blob
            .bound_samplers
            .iter()
            .map(|b| {
                let description = pipeline_description
                    .samplers
                    .get(b.binding as usize)
                    .ok_or(FFX_ERROR_INVALID_ARGUMENT)?;
                Ok((
                    FFX_WGSL_BINDING_OFFSET_SAMPLER + b.binding,
                    self.device.create_sampler(&sampler(description)?),
                ))
            })
            .collect::<Result<Vec<_>, FfxErrorCode>>()?;
        let grid_group = indirect.then(|| {
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("SDK-P10 dispatch grid"),
                layout: &self.grid.layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.grid.arguments.as_entire_binding(),
                }],
            })
        });
        let constants = blob
            .bound_constant_buffers
            .iter()
            .map(|b| {
                let binding = FFX_WGSL_BINDING_OFFSET_CBV + b.binding;
                let size = constant_sizes
                    .get(&binding)
                    .ok_or(FFX_ERROR_BACKEND_API_ERROR)?;
                let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(b.name),
                    size: size.div_ceil(4) * 4,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                Ok((binding, buffer))
            })
            .collect::<Result<Vec<_>, FfxErrorCode>>()?;
        let record = Pipeline {
            effect,
            pass,
            pipeline,
            layout,
            entries,
            constant_sizes,
            constants,
            samplers,
            grid_group,
            groups: RefCell::new(Vec::new()),
        };
        let handle = if let Some(index) = self.pipelines.iter().position(Option::is_none) {
            self.pipelines[index] = Some(record);
            index + 1
        } else {
            self.pipelines.push(Some(record));
            self.pipelines.len()
        };
        // One binding per register and array index, as the DX12 backend
        // expands the blob's binding counts.
        let expand = |table: &[FfxShaderBlobBinding]| {
            table
                .iter()
                .flat_map(|b| {
                    (0..b.count).map(|array_index| FfxResourceBinding {
                        slot_index: b.binding,
                        array_index,
                        resource_identifier: 0,
                        name: b.name.into(),
                    })
                })
                .collect::<Vec<_>>()
        };
        Ok(FfxPipelineState {
            root_signature: handle,
            pass_id: pass,
            cmd_signature: if indirect { handle } else { 0 },
            pipeline: handle,
            uav_texture_bindings: expand(blob.bound_uav_textures),
            srv_texture_bindings: expand(blob.bound_srv_textures),
            srv_buffer_bindings: expand(blob.bound_srv_buffers),
            uav_buffer_bindings: expand(blob.bound_uav_buffers),
            constant_buffer_bindings: expand(blob.bound_constant_buffers),
            name: pipeline_description.name.clone(),
            ..Default::default()
        })
    }

    fn get_permutation_blob_by_index(
        &self,
        effect_id: FfxEffect,
        pass_id: FfxPass,
        bind_stage: FfxBindStage,
        permutation_options: u32,
    ) -> Result<FfxShaderBlob, FfxErrorCode> {
        ffx_get_permutation_blob_by_index(effect_id, pass_id, bind_stage, permutation_options)
    }

    fn destroy_pipeline(
        &mut self,
        pipeline: &mut FfxPipelineState,
        _effect_context_id: u32,
    ) -> Result<(), FfxErrorCode> {
        if let Some(slot) = pipeline
            .pipeline
            .checked_sub(1)
            .and_then(|index| self.pipelines.get_mut(index))
        {
            *slot = None;
        }
        pipeline.pipeline = 0;
        pipeline.root_signature = 0;
        pipeline.cmd_signature = 0;
        Ok(())
    }

    fn schedule_gpu_job(&mut self, job: &FfxGpuJobDescription) -> Result<(), FfxErrorCode> {
        self.jobs.push(job.clone());
        Ok(())
    }

    fn execute_gpu_jobs(
        &mut self,
        command_list: FfxCommandList,
        _effect_context_id: u32,
    ) -> Result<(), FfxErrorCode> {
        let slot = command_list
            .checked_sub(1)
            .filter(|index| self.command_lists.get(*index).is_some_and(Option::is_some))
            .ok_or(FFX_ERROR_INVALID_ARGUMENT)?;
        let mut encoder = self.command_lists[slot].take().unwrap();
        let mut observer = self.observer.take();
        let jobs = std::mem::take(&mut self.jobs);
        let (upload, constants) = self.stage_constants(&jobs);
        // The first failing job stops the rest (SDK-P28), so the encoder
        // holds nothing it could not record.
        let mut failure = None;
        for (job, constants) in jobs.iter().zip(&constants) {
            if let FfxGpuJobDescriptor::Compute(compute) = &job.descriptor
                && let Err(error) = self.back_buffer_bindings(&mut encoder, compute)
            {
                failure = Some((job, JobFailure::from(error)));
                break;
            }
            self.observe(&mut observer, &mut encoder, job, false);
            let result = match &job.descriptor {
                FfxGpuJobDescriptor::ClearFloat(clear) => self.execute_clear(&mut encoder, clear),
                FfxGpuJobDescriptor::Copy(copy) => self
                    .execute_copy(&mut encoder, copy)
                    .map_err(JobFailure::from),
                FfxGpuJobDescriptor::Compute(compute) => {
                    self.execute_compute(&mut encoder, compute, upload.as_ref(), constants)
                }
            };
            if let Err(error) = result {
                failure = Some((job, error));
                break;
            }
            self.observe(&mut observer, &mut encoder, job, true);
        }
        self.observer = observer;
        self.command_lists[slot] = Some(encoder);
        let Some((job, failure)) = failure else {
            return Ok(());
        };
        self.job_error.get_or_insert_with(|| FfxWgpuJobError {
            code: failure.code,
            message: failure
                .rejection
                .unwrap_or_else(|| format!("the backend refused job '{}'", job.job_label)),
        });
        Err(failure.code)
    }
}

/// `GetFfxResourceDescriptionDX12` for a wgpu texture.
pub fn ffx_get_resource_description_wgpu(texture: &wgpu::Texture) -> FfxResourceDescription {
    let usage = texture.usage();
    let mut ffx_usage = FFX_RESOURCE_USAGE_READ_ONLY;
    if usage.contains(wgpu::TextureUsages::STORAGE_BINDING) {
        ffx_usage |= FFX_RESOURCE_USAGE_UAV;
    }
    if usage.contains(wgpu::TextureUsages::RENDER_ATTACHMENT) {
        ffx_usage |= if texture.format().is_depth_stencil_format() {
            FFX_RESOURCE_USAGE_DEPTHTARGET
        } else {
            FFX_RESOURCE_USAGE_RENDERTARGET
        };
    }
    FfxResourceDescription {
        r#type: match texture.dimension() {
            wgpu::TextureDimension::D1 => FfxResourceType::Texture1D,
            wgpu::TextureDimension::D2 => FfxResourceType::Texture2D,
            wgpu::TextureDimension::D3 => FfxResourceType::Texture3D,
        },
        format: surface_format(texture.format()),
        width: texture.width(),
        height: texture.height(),
        depth: texture.depth_or_array_layers(),
        mip_count: texture.mip_level_count(),
        flags: FFX_RESOURCE_FLAGS_NONE,
        usage: ffx_usage,
    }
}

/// The wgpu object for a resource description, and its size in bytes.
fn create_object(
    device: &wgpu::Device,
    d: &FfxCreateResourceDescription,
) -> Result<(Object, u64), FfxErrorCode> {
    let description = d.resource_description;
    if description.r#type == FfxResourceType::Buffer {
        // `width` is the buffer size; storage buffers are whole words.
        let size = u64::from(description.width.max(1)).next_multiple_of(4);
        let mut usage = wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST;
        if description.usage & FFX_RESOURCE_USAGE_INDIRECT != 0 {
            usage |= wgpu::BufferUsages::INDIRECT;
        }
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(d.name),
            size,
            usage,
            mapped_at_creation: false,
        });
        return Ok((Object::Buffer(buffer), size));
    }
    let format = storage_texture_format(&description)?;
    let (dimension, layers) = match description.r#type {
        FfxResourceType::Texture1D => (wgpu::TextureDimension::D1, 1),
        FfxResourceType::Texture2D => (wgpu::TextureDimension::D2, 1),
        FfxResourceType::TextureCube => (wgpu::TextureDimension::D2, 6),
        FfxResourceType::Texture3D => (wgpu::TextureDimension::D3, description.depth.max(1)),
        FfxResourceType::Buffer => unreachable!(),
    };
    let size = wgpu::Extent3d {
        width: description.width,
        height: description.height,
        depth_or_array_layers: layers,
    };
    let mip_level_count = if description.mip_count == 0 {
        size.max_mips(dimension)
    } else {
        description.mip_count
    };
    let mut usage = wgpu::TextureUsages::TEXTURE_BINDING
        | wgpu::TextureUsages::COPY_SRC
        | wgpu::TextureUsages::COPY_DST;
    if description.usage & FFX_RESOURCE_USAGE_UAV != 0 {
        usage |= wgpu::TextureUsages::STORAGE_BINDING;
        // The SDK's `Interlocked*` on 32-bit integer UAVs (FSR2's
        // reconstructed depth) are WGSL texture atomics, which wgpu's Metal
        // backend only performs on a texture created for them.
        if matches!(
            format,
            wgpu::TextureFormat::R32Uint | wgpu::TextureFormat::R32Sint
        ) {
            usage |= wgpu::TextureUsages::STORAGE_ATOMIC;
        }
    }
    if description.usage & (FFX_RESOURCE_USAGE_RENDERTARGET | FFX_RESOURCE_USAGE_DEPTHTARGET) != 0 {
        usage |= wgpu::TextureUsages::RENDER_ATTACHMENT;
    }
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(d.name),
        size,
        mip_level_count,
        sample_count: 1,
        dimension,
        format,
        usage,
        view_formats: &[],
    });
    let block = u64::from(format.block_copy_size(None).unwrap_or(0));
    let bytes = (0..mip_level_count)
        .map(|mip| {
            let level = size.mip_level_size(mip, dimension);
            u64::from(level.width)
                * u64::from(level.height)
                * u64::from(level.depth_or_array_layers)
                * block
        })
        .sum();
    Ok((Object::Texture(texture), bytes))
}

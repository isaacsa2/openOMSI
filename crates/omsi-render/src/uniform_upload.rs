//! Skip identical writes to a uniform buffer. This remembers its last upload, not the
//! last value of one view: a mirror or a triple-screen panel may change it in between.

use std::cell::Cell;

pub(crate) struct UniformUpload<T: bytemuck::Pod> {
    sent: Cell<Option<T>>,
}

impl<T: bytemuck::Pod> Default for UniformUpload<T> {
    fn default() -> Self {
        Self {
            sent: Cell::new(None),
        }
    }
}

impl<T: bytemuck::Pod> UniformUpload<T> {
    fn changed(&self, value: T) -> bool {
        if self
            .sent
            .get()
            .is_some_and(|old| bytemuck::bytes_of(&old) == bytemuck::bytes_of(&value))
        {
            return false;
        }
        self.sent.set(Some(value));
        true
    }

    pub(crate) fn write(&self, queue: &wgpu::Queue, buffer: &wgpu::Buffer, value: T) {
        if self.changed(value) {
            queue.write_buffer(buffer, 0, bytemuck::bytes_of(&value));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn another_view_and_returning_to_the_first_both_upload() {
        let upload = UniformUpload::default();
        let a = [1017.0_f32, 555.0, 0.3, 0.0];
        let b = [1197.0_f32, 653.0, 0.0, 1.0];
        assert!(upload.changed(a));
        assert!(!upload.changed(a));
        assert!(upload.changed(b));
        assert!(upload.changed(a));
        assert!(!upload.changed(a));
        assert!(UniformUpload::default().changed(a)); // a new renderer/buffer starts empty
    }

    #[test]
    fn compares_gpu_bytes_including_nan_payloads_and_signed_zero() {
        let upload = UniformUpload::default();
        let nan = f32::from_bits(0x7fc0_0001);
        assert!(upload.changed([0.0_f32, nan]));
        assert!(!upload.changed([0.0_f32, nan]));
        assert!(upload.changed([-0.0_f32, nan]));
        assert!(upload.changed([-0.0_f32, f32::from_bits(0x7fc0_0002)]));
    }
}

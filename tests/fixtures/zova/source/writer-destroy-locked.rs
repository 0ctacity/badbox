    fn destroy_locked(&mut self) {
        if let Some(raw) = self.raw.take() {
            unsafe {
                let _ = zova_sys::zova_object_writer_destroy(raw.as_ptr());
            }
        }
    }

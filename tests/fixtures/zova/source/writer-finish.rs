    pub fn finish(mut self) -> Result<ObjectId> {
        let raw = self.raw()?;
        let database = self.database.clone();
        let _guard = database.lock();
        let mut out = zova_sys::zova_object_id { bytes: [0; 32] };
        let request = zova_sys::zova_object_writer_finish_request {
            writer: raw.as_ptr(),
            out_id: &mut out,
        };
        database.status_locked(unsafe { zova_sys::zova_object_writer_finish(&request) })?;
        self.destroy_locked();
        Ok(from_c_object_id(out))
    }

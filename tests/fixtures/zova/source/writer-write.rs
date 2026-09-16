    pub fn write(&mut self, bytes: &[u8]) -> Result<()> {
        let raw = self.raw()?;
        let database = self.database.clone();
        let _guard = database.lock();
        let request = zova_sys::zova_object_writer_write_request {
            writer: raw.as_ptr(),
            data: bytes.as_ptr(),
            len: bytes.len(),
        };
        database.status_locked(unsafe { zova_sys::zova_object_writer_write(&request) })
    }

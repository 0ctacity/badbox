    pub fn read(&mut self, buffer: &mut [u8]) -> Result<usize> {
        let raw = self.raw()?;
        let database = self.database.clone();
        let _guard = database.lock();
        let mut read = 0;
        let request = zova_sys::zova_object_reader_read_request {
            reader: raw.as_ptr(),
            buffer: buffer.as_mut_ptr(),
            buffer_len: buffer.len(),
            out_read: &mut read,
        };
        database.status_locked(unsafe { zova_sys::zova_object_reader_read(&request) })?;
        Ok(read)
    }

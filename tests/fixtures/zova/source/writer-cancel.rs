    pub fn cancel(mut self) -> Result<()> {
        let raw = self.raw()?;
        let database = self.database.clone();
        let _guard = database.lock();
        let request = zova_sys::zova_object_writer_cancel_request {
            writer: raw.as_ptr(),
        };
        database.status_locked(unsafe { zova_sys::zova_object_writer_cancel(&request) })?;
        self.destroy_locked();
        Ok(())
    }

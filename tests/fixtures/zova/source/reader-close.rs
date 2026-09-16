    pub fn close(&mut self) -> Result<()> {
        if self.raw.is_none() {
            return Ok(());
        }
        let database = self.database.clone();
        let _guard = database.lock();
        self.destroy_locked(true)
    }

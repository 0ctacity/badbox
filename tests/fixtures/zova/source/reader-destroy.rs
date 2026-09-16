    fn destroy(&mut self) {
        if self.raw.is_none() {
            return;
        }
        let database = self.database.clone();
        let _guard = database.lock();
        let _ = self.destroy_locked(false);
    }

    fn destroy(&mut self) {
        if self.raw.is_none() {
            return;
        }
        let database = self.database.clone();
        let _guard = database.lock();
        self.destroy_locked();
    }

    pub fn kv_get(&mut self, namespace: &[u8], key: &[u8]) -> Result<Option<Vec<u8>>> {
        let db = self.raw_ptr();
        let status = |status| self.status(status);
        kv_get_raw(db, status, namespace, key)
    }

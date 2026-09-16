    pub fn kv_put(&mut self, namespace: &[u8], key: &[u8], value: &[u8]) -> Result<()> {
        let db = self.raw_ptr();
        let status = |status| self.status(status);
        kv_put_raw(db, status, namespace, key, value)
    }

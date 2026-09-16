    pub fn kv_get_many(
        &mut self,
        namespace: &[u8],
        keys: &[&[u8]],
    ) -> Result<Vec<Option<Vec<u8>>>> {
        let db = self.raw_ptr();
        let status = |status| self.status(status);
        kv_get_many_raw(db, status, namespace, keys)
    }

    pub fn listen(&self, channel: &str) -> Result<SharedSubscription> {
        let _guard = self.inner.lock();
        let raw = listen_raw(self.inner.raw_ptr(), channel)?;
        Ok(SharedSubscription {
            raw: Some(raw),
            database: self.inner.clone(),
            _not_sync: PhantomData,
        })
    }

    pub fn listen(&mut self, channel: &str) -> Result<Subscription> {
        let raw = listen_raw(self.raw_ptr(), channel)?;
        Ok(Subscription::new(raw, self.inner.clone()))
    }

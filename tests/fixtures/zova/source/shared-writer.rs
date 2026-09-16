    pub fn object_writer(&self) -> Result<SharedObjectWriter> {
        let _guard = self.inner.lock();
        let mut writer = ptr::null_mut();
        let request = zova_sys::zova_object_writer_create_request {
            db: self.inner.raw_ptr(),
            out_writer: &mut writer,
        };
        self.inner
            .status_locked(unsafe { zova_sys::zova_object_writer_create(&request) })?;
        let raw = NonNull::new(writer)
            .ok_or_else(|| Error::from_status(zova_sys::ZOVA_INVALID_ARGUMENT, None))?;
        Ok(SharedObjectWriter {
            raw: Some(raw),
            database: self.inner.clone(),
            _not_sync: PhantomData,
        })
    }

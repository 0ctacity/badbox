    pub fn object_reader(&self, id: ObjectId) -> Result<SharedObjectReader> {
        let _guard = self.inner.lock();
        let mut reader = ptr::null_mut();
        let request = zova_sys::zova_object_reader_create_request {
            db: self.inner.raw_ptr(),
            id: id.to_c(),
            out_reader: &mut reader,
        };
        self.inner
            .status_locked(unsafe { zova_sys::zova_object_reader_create(&request) })?;
        let raw = NonNull::new(reader)
            .ok_or_else(|| Error::from_status(zova_sys::ZOVA_INVALID_ARGUMENT, None))?;
        Ok(SharedObjectReader {
            raw: Some(raw),
            database: self.inner.clone(),
            _not_sync: PhantomData,
        })
    }

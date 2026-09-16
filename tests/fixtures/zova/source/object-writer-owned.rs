    pub fn object_writer_owned(&mut self) -> Result<OwnedObjectWriter> {
        let mut writer = ptr::null_mut();
        let request = zova_sys::zova_object_writer_create_request {
            db: self.raw_ptr(),
            out_writer: &mut writer,
        };
        self.status(unsafe { zova_sys::zova_object_writer_create(&request) })?;
        let raw = NonNull::new(writer)
            .ok_or_else(|| Error::from_status(zova_sys::ZOVA_INVALID_ARGUMENT, None))?;
        Ok(OwnedObjectWriter {
            raw: Some(raw),
            db: self.raw_ptr(),
            _database: self.inner.clone(),
            _not_send_sync: PhantomData,
        })
    }

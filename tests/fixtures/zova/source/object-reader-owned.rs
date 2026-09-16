    pub fn object_reader_owned(&mut self, id: ObjectId) -> Result<OwnedObjectReader> {
        let mut reader = ptr::null_mut();
        let request = zova_sys::zova_object_reader_create_request {
            db: self.raw_ptr(),
            id: id.to_c(),
            out_reader: &mut reader,
        };
        self.status(unsafe { zova_sys::zova_object_reader_create(&request) })?;
        let raw = NonNull::new(reader)
            .ok_or_else(|| Error::from_status(zova_sys::ZOVA_INVALID_ARGUMENT, None))?;
        Ok(OwnedObjectReader {
            raw: Some(raw),
            db: self.raw_ptr(),
            _database: self.inner.clone(),
            _not_send_sync: PhantomData,
        })
    }

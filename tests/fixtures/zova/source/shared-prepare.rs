    pub fn prepare(&self, sql: &str) -> Result<SharedStatement> {
        let sql = cstring(sql, "sql")?;
        let _guard = self.inner.lock();
        let mut statement = ptr::null_mut();
        let request = zova_sys::zova_database_prepare_request {
            db: self.inner.raw_ptr(),
            sql: sql.as_ptr(),
            out_statement: &mut statement,
        };
        self.inner
            .status_locked(unsafe { zova_sys::zova_database_prepare(&request) })?;
        let raw = NonNull::new(statement)
            .ok_or_else(|| Error::from_status(zova_sys::ZOVA_INVALID_ARGUMENT, None))?;
        Ok(SharedStatement {
            raw: Some(raw),
            database: self.inner.clone(),
            _not_sync: PhantomData,
        })
    }

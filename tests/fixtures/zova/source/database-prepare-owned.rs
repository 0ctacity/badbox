    pub fn prepare_owned(&mut self, sql: &str) -> Result<OwnedStatement> {
        let sql = cstring(sql, "sql")?;
        let mut statement = ptr::null_mut();
        let request = zova_sys::zova_database_prepare_request {
            db: self.raw_ptr(),
            sql: sql.as_ptr(),
            out_statement: &mut statement,
        };
        self.status(unsafe { zova_sys::zova_database_prepare(&request) })?;
        let raw = NonNull::new(statement)
            .ok_or_else(|| Error::from_status(zova_sys::ZOVA_INVALID_ARGUMENT, None))?;
        Ok(OwnedStatement::new(raw, self.inner.clone()))
    }

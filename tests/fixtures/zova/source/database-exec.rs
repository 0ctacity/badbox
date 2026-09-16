    pub fn exec(&mut self, sql: &str) -> Result<()> {
        let sql = cstring(sql, "sql")?;
        let request = zova_sys::zova_database_exec_request {
            db: self.raw_ptr(),
            sql: sql.as_ptr(),
        };
        self.status(unsafe { zova_sys::zova_database_exec(&request) })
    }

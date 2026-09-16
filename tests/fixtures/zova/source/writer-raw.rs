    fn raw(&self) -> Result<NonNull<zova_sys::zova_object_writer>> {
        self.raw
            .ok_or_else(|| Error::from_status(zova_sys::ZOVA_OBJECT_WRITER_CLOSED, None))
    }

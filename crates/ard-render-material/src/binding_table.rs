#[derive(Default)]
pub struct BindingTableOffset {
    /// Current binding table offset
    bt_offset: u32,
}

impl BindingTableOffset {
    #[inline(always)]
    pub fn binding_table_size(&self) -> usize {
        self.bt_offset as usize
    }

    // Registers a new shader for the pass. Returns the index of said shader in a binding table.
    #[inline(always)]
    pub fn register(&mut self) -> u32 {
        let ret = self.bt_offset;
        self.bt_offset += 1;
        ret
    }
}

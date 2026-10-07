use crate::builder::Ext4ImageBuilder;

impl Ext4ImageBuilder {
    pub fn is_inode_allocated(&self, ino: u32) -> bool {
        let inodes_per_group = self.superblock.s_inodes_per_group as usize;
        let group = (ino - 1) / inodes_per_group as u32;
        let index_in_group = (ino - 1) % inodes_per_group as u32;
        let byte_idx = (index_in_group / 8) as usize;
        let bit_idx = index_in_group % 8;

        self.groups[group as usize].inode_bitmap[byte_idx] & (1 << bit_idx) != 0
    }

    pub fn allocate_inode(&mut self, parent_dir_ino: Option<u32>) -> Result<u32, String> {
        let total_inodes = self.superblock.s_inodes_count;
        let inodes_per_group = self.superblock.s_inodes_per_group;
        let start_ino = match parent_dir_ino {
            Some(dir) if dir > 0 && dir <= total_inodes => {
                ((dir - 1) / inodes_per_group) * inodes_per_group + 1
            }
            _ => 1,
        };

        let ino = (start_ino..=total_inodes)
            .chain(1..start_ino)
            .find(|&ino| !self.is_inode_allocated(ino))
            .ok_or_else(|| "No free inodes available".to_string())?;

        let group_idx = ((ino - 1) / inodes_per_group) as usize;
        self.mark_inode_used(group_idx, ino);
        Ok(ino)
    }
}

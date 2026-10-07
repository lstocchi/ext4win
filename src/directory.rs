use zerocopy::{FromBytes, IntoBytes};

use crate::{builder::Ext4ImageBuilder, ext4::*};

impl Ext4ImageBuilder {
    pub fn add_dir_entry(&mut self, parent_ino: u32, name: &str, child_ino: u32, file_type: u8) {
        let inodes_per_group = self.superblock.s_inodes_per_group;
        let parent_group = ((parent_ino - 1) / inodes_per_group) as usize;
        let parent_index = ((parent_ino - 1) % inodes_per_group) as usize;
        let extent = Ext4Extent::read_from_bytes(
            &self.groups[parent_group].inode_table[parent_index].i_block[12..24],
        )
        .unwrap();
        let block_offset = extent.ee_start_lo as usize * BLOCK_SIZE;
        let name_bytes = name.as_bytes();
        let name_len = name_bytes.len() as u8;
        let needed_len = ((8 + name_len as u16 + 3) / 4) * 4;

        let mut offset = 0;
        while offset < BLOCK_SIZE {
            let entry = Ext4DirEntry2Header::read_from_bytes(
                &self.disk_data[block_offset + offset..block_offset + offset + 8],
            )
            .unwrap();
            let rec_len = entry.rec_len as usize;
            if offset + rec_len == BLOCK_SIZE {
                let actual_len = ((8 + entry.name_len as usize + 3) / 4) * 4;
                if rec_len - actual_len < needed_len as usize {
                    panic!("Not enough space in the directory block to add entry for {name}");
                }

                self.disk_data[block_offset + offset + 4..block_offset + offset + 6]
                    .copy_from_slice(&(actual_len as u16).to_le_bytes());
                let new_offset = offset + actual_len;
                let new_entry = Ext4DirEntry2Header {
                    inode: child_ino,
                    rec_len: (BLOCK_SIZE - new_offset) as u16,
                    name_len,
                    file_type,
                };
                self.disk_data[block_offset + new_offset..block_offset + new_offset + 8]
                    .copy_from_slice(new_entry.as_bytes());
                self.disk_data[block_offset + new_offset + 8
                    ..block_offset + new_offset + 8 + name_len as usize]
                    .copy_from_slice(name_bytes);
                return;
            }
            offset += rec_len;
        }
        panic!("invalid directory block for inode {parent_ino}");
    }

    pub fn mkdir(&mut self, parent_ino: u32, name: &str) -> Result<u32, String> {
        if name.is_empty()
            || name == "."
            || name == ".."
            || name.len() > u8::MAX as usize
            || name.contains('/')
        {
            return Err(
                "directory name must be a non-empty component no longer than 255 bytes".to_string(),
            );
        }
        if parent_ino == 0
            || parent_ino > self.superblock.s_inodes_count
            || !self.is_inode_allocated(parent_ino)
        {
            return Err(format!("parent inode {parent_ino} does not exist"));
        }

        let inodes_per_group = self.superblock.s_inodes_per_group;
        let parent_group = ((parent_ino - 1) / inodes_per_group) as usize;
        let parent_index = ((parent_ino - 1) % inodes_per_group) as usize;
        let parent_mode = self.groups[parent_group].inode_table[parent_index].i_mode;
        if parent_mode & S_IFDIR != S_IFDIR {
            return Err(format!("parent inode {parent_ino} is not a directory"));
        }

        let dir_ino = self.allocate_inode(Some(parent_ino))?;
        let group = ((dir_ino - 1) / inodes_per_group) as usize;
        let index = ((dir_ino - 1) % inodes_per_group) as usize;
        let data_block = self.allocate_free_block(group);
        let mut dir_block = vec![0; BLOCK_SIZE];
        let dot = Ext4DirEntry2Header {
            inode: dir_ino,
            rec_len: 12,
            name_len: 1,
            file_type: EXT4_FT_DIR,
        };
        let dotdot = Ext4DirEntry2Header {
            inode: parent_ino,
            rec_len: (BLOCK_SIZE - 12) as u16,
            name_len: 2,
            file_type: EXT4_FT_DIR,
        };
        dir_block[..8].copy_from_slice(dot.as_bytes());
        dir_block[8] = b'.';
        dir_block[12..20].copy_from_slice(dotdot.as_bytes());
        dir_block[20..22].copy_from_slice(b"..");
        let disk_offset = data_block as usize * BLOCK_SIZE;
        self.disk_data[disk_offset..disk_offset + BLOCK_SIZE].copy_from_slice(&dir_block);

        let mut inode = Ext4Inode::default();
        inode.i_mode = S_IFDIR | 0o755;
        inode.i_size_lo = BLOCK_SIZE as u32;
        inode.i_links_count = 2;
        inode.i_blocks_lo = (BLOCK_SIZE / 512) as u32;
        inode.i_flags = EXT4_EXTENTS_FL;
        let header = Ext4ExtentHeader {
            eh_magic: EXT4_EH_MAGIC,
            eh_entries: 1,
            eh_max: 4,
            eh_depth: 0,
            eh_generation: 0,
        };
        let extent = Ext4Extent {
            ee_block: 0,
            ee_len: 1,
            ee_start_hi: 0,
            ee_start_lo: data_block,
        };
        inode.i_block[..12].copy_from_slice(header.as_bytes());
        inode.i_block[12..24].copy_from_slice(extent.as_bytes());
        self.groups[group].inode_table[index] = inode;
        self.group_descriptors[group].bg_used_dirs_count_lo += 1;
        self.add_dir_entry(parent_ino, name, dir_ino, EXT4_FT_DIR);
        self.groups[parent_group].inode_table[parent_index].i_links_count += 1;
        Ok(dir_ino)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mkdir_creates_a_reachable_directory_without_double_accounting() {
        let mut builder = Ext4ImageBuilder::new(16);
        builder.create_root_dir();
        builder.create_lost_and_found();
        builder.reserve_inodes();
        let free_before = builder.superblock.s_free_inodes_count;
        let child_ino = builder.mkdir(EXT4_ROOT_INO, "projects").unwrap();

        assert!(builder.is_inode_allocated(child_ino));
        let free_after = builder.superblock.s_free_inodes_count;
        let root_links = builder.groups[0].inode_table[1].i_links_count;
        assert_eq!(free_after, free_before - 1);
        assert_eq!(root_links, 4);

        let root_extent =
            Ext4Extent::read_from_bytes(&builder.groups[0].inode_table[1].i_block[12..24]).unwrap();
        let root_offset = root_extent.ee_start_lo as usize * BLOCK_SIZE;
        let mut offset = 0;
        let mut found = false;
        while offset < BLOCK_SIZE {
            let entry = Ext4DirEntry2Header::read_from_bytes(
                &builder.disk_data[root_offset + offset..root_offset + offset + 8],
            )
            .unwrap();
            let start = root_offset + offset + 8;
            if &builder.disk_data[start..start + entry.name_len as usize] == b"projects" {
                let inode = entry.inode;
                assert_eq!(inode, child_ino);
                found = true;
                break;
            }
            offset += entry.rec_len as usize;
        }
        assert!(found);
        builder.write_to_file("target/mkdir-test.img").unwrap();
    }
}

mod builder;
mod ext4;

use builder::Ext4ImageBuilder;

fn main() -> std::io::Result<()> {
    const IMAGE_SIZE_MB: usize = 400;
    let output_file = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "output.img".to_owned());

    println!("Creating {IMAGE_SIZE_MB}MB ext4 filesystem image...");
    let mut builder = Ext4ImageBuilder::new(IMAGE_SIZE_MB);

    builder.create_root_dir();
    builder.create_lost_and_found();
    builder.reserve_inodes();

    builder.write_to_file(&output_file)?;

    println!("Ext4 image successfully generated at: {}", output_file);
    Ok(())
}

use anyhow::Result;
use bia_importer::{run, setup_connection, Args};

fn main() -> Result<()> {
    let args = <Args as clap::Parser>::parse();
    let mut conn = setup_connection(&args.db)?;
    run(&mut conn, &args, &mut |_| {})?;
    Ok(())
}

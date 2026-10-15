//! upright [--flip] <cutout.png> <out.png>: stand the pen in a cutout upright (`thepenmarket::upright`).
//! Exit 0 and a file written: done. Exit 3: not a pen-shaped photo, nothing written.
fn main() -> anyhow::Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let flip = args.first().map(|a| a == "--flip").unwrap_or(false);
    if flip {
        args.remove(0);
    }
    if args.len() != 2 {
        anyhow::bail!("usage: upright [--flip] <cutout.png> <out.png>");
    }
    if thepenmarket::upright::make_upright_file(std::path::Path::new(&args[0]), std::path::Path::new(&args[1]), flip)? {
        Ok(())
    } else {
        std::process::exit(3)
    }
}

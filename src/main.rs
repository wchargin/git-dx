use std::io;
use std::process::Command;

fn main() -> io::Result<()> {
    let out = Command::new("git")
        .args(&["rev-parse", "--verify", "HEAD^{commit}"])
        .output()?;
    if !out.status.success() {
        eprintln!(
            "Failed to parse commit: {:?}",
            String::from_utf8_lossy(&out.stderr)
        );
        std::process::exit(1)
    }
    let oid = parse_oid(out.stdout).expect("successful rev-parse should give valid hash");
    println!("Head is at {:?}", oid);
    Ok(())
}

fn parse_oid(stdout: Vec<u8>) -> Result<String, Vec<u8>> {
    let mut raw = String::from_utf8(stdout).map_err(|e| e.into_bytes())?;
    match raw.pop() {
        Some('\n') => return Ok(raw),
        Some(other) => raw.push(other),
        None => (),
    }
    Err(raw.into_bytes())
}

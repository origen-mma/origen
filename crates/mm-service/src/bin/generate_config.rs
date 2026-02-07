use mm_config::Config;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Generating default configuration...");

    let config = Config::development();
    config.save("config/config.example.toml")?;

    println!("Configuration template saved to: config/config.example.toml");
    println!("\nTo use:");
    println!("1. Copy config/config.example.toml to config/config.toml");
    println!("2. Edit config/config.toml and add your credentials");
    println!("3. Run: cargo run --bin mm-service");
    println!("\nOr set environment variables:");
    println!("  export GCN_CLIENT_ID=your_client_id");
    println!("  export GCN_CLIENT_SECRET=your_client_secret");
    println!("  export BOOM_SASL_USERNAME=your_username");
    println!("  export BOOM_SASL_PASSWORD=your_password");

    Ok(())
}

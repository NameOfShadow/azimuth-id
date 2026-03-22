fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = tonic_prost_build::configure();
    
    // Добавляем serde-деривы если включена фича
    #[cfg(feature = "serde")]
    {
        config = config
            .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]");
    }
    
    config.compile_protos(&["proto/auth.proto"], &["proto"])?;
    
    println!("cargo:rerun-if-changed=proto/auth.proto");
    Ok(())
}
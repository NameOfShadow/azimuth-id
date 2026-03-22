# azimuth-client

gRPC клиент для взаимодействия с Azimuth Auth Service.

## Установка

```toml
[dependencies]
azimuth-client = { git = "https://github.com/NameOfShadow/azimuth-auth.git", branch = "main" }
```

## Пример использования

```rust
use azimuth_client::AuthClient;

#[tokio::main]
async fn main() {
    let client = AuthClient::connect("http://localhost:50051", "my-service").await?;
    
    let user = client.verify_token("eyJhbGciOiJIUzI1NiIs...").await?;
    println!("Authenticated user: {}", user.username);
    
    Ok(())
}
```

## Лицензия

MIT
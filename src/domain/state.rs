use crate::config::Config;
use crate::domain::{
    JwtService, CaptchaService, EmailService, EmailConfig,
    VerificationCodeService, VerificationCodeConfig,
};

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    pub config: Config,
    pub jwt: JwtService,
    pub captcha: Option<CaptchaService>,
    pub email: Option<EmailService>,
    pub verification_code: Option<VerificationCodeService>,
}

impl AppState {
    pub async fn new(db: sqlx::PgPool, config: Config) -> Result<Self, String> {
        let jwt = JwtService::new(config.jwt_secret.clone(), config.jwt_expiration_days);
        
        let captcha = config.turnstile_secret.clone()
            .map(|secret| CaptchaService::new(secret));
        
        let email = if config.email_enabled() {
            let email_config = EmailConfig {
                from_email: config.from_email.clone(),
                from_name: config.from_name.clone(),
                base_url: config.base_url.clone(),
                resend_api_key: config.resend_api_key.clone(),
            };
            Some(EmailService::new(email_config))
        } else {
            None
        };

        let verification_code = if config.verification_code_enabled() {
            let code_config = VerificationCodeConfig {
                redis_url: config.redis_url.clone()
                    .unwrap_or_else(|| "redis://localhost:6379".into()),
                code_length: 6,
                ttl_minutes: 15,
                max_attempts: 3,
            };
            Some(VerificationCodeService::new(code_config).await?)
        } else {
            None
        };
        
        Ok(Self { 
            db, 
            config, 
            jwt, 
            captcha, 
            email,
            verification_code,
        })
    }
    
    pub fn user_repo(&self) -> crate::domain::UserRepository<'_> {
        crate::domain::UserRepository::new(&self.db)
    }
}
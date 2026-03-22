#![allow(clippy::all)]

pub mod azimuth {
    pub mod auth {
        pub mod v1 {
            tonic::include_proto!("azimuth.auth.v1");
        }
    }
}
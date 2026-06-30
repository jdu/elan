//! Generated gRPC stubs for all elan proto services.
//!
//! Each sub-module is produced by `tonic_build` at compile time from the
//! corresponding `.proto` file in the workspace `proto/` directory.

pub mod catalog {
    tonic::include_proto!("elan.catalog");
}

pub mod coordinator {
    tonic::include_proto!("elan.coordinator");
}

pub mod iam {
    tonic::include_proto!("elan.iam");
}

pub mod audit {
    tonic::include_proto!("elan.audit");
}

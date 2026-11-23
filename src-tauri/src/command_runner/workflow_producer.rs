//! Reviewed azd-pim source pin for offline qualification.
use crate::{
    component_provenance::{bounded_bytes, Root},
    error::{AppError, AppResult},
};
use sha2::{Digest, Sha256};
use std::path::Path;

pub(super) const REVISION: &str = "5d22767595740d5e3d1aafccecf3435bf0e9b9fc";
pub(super) const ENTRYPOINT: &str = "scripts/Post-Provision.Native.ps1";

// Raw Git bytes for the revocation-only producer, including the azd hook
// declaration and both files of the vendored Graph authentication module.
// The optional polling deployment and external Graph SDK/CLI are not enrolled.
pub(super) const ASSETS: &[(&str, u64, &str)] = &[
    (
        "azure.yaml",
        479,
        "6630471554d43bf311f39d8f5ed3e1891363bda1a1f86d9ae0fa8c3133054589",
    ),
    (
        ENTRYPOINT,
        617,
        "94117df6eb860995f8fed952737269a12836041b21d289182440477c33f3d2df",
    ),
    (
        "scripts/Post-Provision.ps1",
        8301,
        "8aaeec99b26c3afa16dd16605bf22f10bd41d6bfe1d4af796dd75dcf2a023726",
    ),
    (
        "scripts/AzdPim.Common.psm1",
        21216,
        "63cccca5fe098586308edc31fba405da492ebcbfd011cc39e87cbacfe2ddd806",
    ),
    (
        "scripts/AzdPim.Graph.psm1",
        66248,
        "6d9fd45b095ce0f1447e34cf7c90fd272a67a94dc07a4069eb14728cd342e60c",
    ),
    (
        "scripts/AzdPim.Optional.psm1",
        37524,
        "15cc21d44c3a44610afad59163984dc70c0b95b6882ead576b385fff746fe7e3",
    ),
    (
        "scripts/AzdPim.Authentication.psm1",
        4556,
        "d8f506e2c2ad0d4d5018921ee98eaf15967f4f43b052b0d096b23e579ee3fa02",
    ),
    (
        "scripts/vendor/Azd.GraphAuthentication/Azd.GraphAuthentication.psd1",
        932,
        "6994ecd6dc301649c9c5c53d43ff39e0a930e61c43e443772e9ee0bafc948729",
    ),
    (
        "scripts/vendor/Azd.GraphAuthentication/Azd.GraphAuthentication.psm1",
        9773,
        "f727f0680a6f11be057d5a0993c6fec9917d7de8870685427a5ec8ca4d739b5e",
    ),
];

/// Return the entry point only after verifying every source file in the pin.
pub(super) fn verify_source(root: &Path) -> AppResult<Vec<u8>> {
    let invalid =
        || AppError::Invalid("The pinned workflow producer files could not be verified.".into());
    let root = Root::open(root).map_err(|_| invalid())?;
    let mut entrypoint = None;
    for &(path, length, digest) in ASSETS {
        let bytes = bounded_bytes(root.file(path).map_err(|_| invalid())?, length)
            .map_err(|_| invalid())?;
        if bytes.len() as u64 != length || hex::encode(Sha256::digest(&bytes)) != digest {
            return Err(invalid());
        }
        if path == ENTRYPOINT {
            entrypoint = Some(bytes);
        }
    }
    entrypoint.ok_or_else(invalid)
}

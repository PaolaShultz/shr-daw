use anyhow::{bail, Context, Result};
use serde::Deserialize;

const CONTRACT: &str = include_str!("../install/compatibility.json");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(not(test), allow(dead_code))]
struct Contract {
    schema_version: u32,
    system_version: String,
    components: Vec<Component>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(not(test), allow(dead_code))]
struct Component {
    name: String,
    version: String,
    repository: Option<String>,
    revision: Option<String>,
    runtime_minimum: Option<String>,
    runtime_maximum_exclusive: Option<String>,
}

pub fn shr_sampler_version_supported(version: &str) -> Result<bool> {
    let contract: Contract =
        serde_json::from_str(CONTRACT).context("parse compatibility contract")?;
    if contract.schema_version != 1 {
        bail!("unsupported compatibility contract schema");
    }
    let sampler = contract
        .components
        .iter()
        .find(|component| component.name == "shr-sampler")
        .context("compatibility contract has no SHR Sampler component")?;
    let minimum = sampler
        .runtime_minimum
        .as_deref()
        .context("SHR Sampler runtime minimum is missing")?;
    let maximum = sampler
        .runtime_maximum_exclusive
        .as_deref()
        .context("SHR Sampler runtime maximum is missing")?;
    Ok(parse_version(version)? >= parse_version(minimum)?
        && parse_version(version)? < parse_version(maximum)?)
}

fn parse_version(version: &str) -> Result<(u64, u64, u64)> {
    let mut parts = version.split('.');
    let parsed = (
        parts.next().context("version major is missing")?.parse()?,
        parts.next().context("version minor is missing")?.parse()?,
        parts.next().context("version patch is missing")?.parse()?,
    );
    if parts.next().is_some() {
        bail!("version must contain exactly three numeric components");
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatibility_contract_is_pinned_and_matches_this_package() {
        let contract: Contract = serde_json::from_str(CONTRACT).unwrap();
        assert_eq!(contract.schema_version, 1);
        assert_eq!(contract.system_version, env!("CARGO_PKG_VERSION"));
        for component in &contract.components {
            assert!(parse_version(&component.version).is_ok());
            if let Some(repository) = &component.repository {
                assert!(repository.starts_with("https://github.com/PaolaShultz/"));
                let revision = component.revision.as_deref().unwrap();
                assert_eq!(revision.len(), 40);
                assert!(revision.bytes().all(|byte| byte.is_ascii_hexdigit()));
            }
        }
        assert!(shr_sampler_version_supported("0.1.2").unwrap());
        assert!(!shr_sampler_version_supported("0.1.1").unwrap());
        assert!(!shr_sampler_version_supported("0.2.0").unwrap());
    }
}

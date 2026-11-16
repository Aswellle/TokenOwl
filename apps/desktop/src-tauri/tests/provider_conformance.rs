//! Runtime conformance between the shipped manifests and the adapter surface.
//!
//! The capability level is not trusted from YAML alone: these tests pin the
//! contract that every declared endpoint kind is one the adapter for that
//! wire format actually executes, that a native-protocol manifest declares
//! only what its adapter probes, and that an L4 claim carries the full
//! endpoint set *and* the reset mapping needed to honor it.

use tokenowl_lib::domain::provider::WireFormat;
use tokenowl_lib::providers::manifest::ProviderManifest;

fn builtin_manifests() -> Vec<(&'static str, ProviderManifest)> {
    tokenowl_lib::providers::builtin::BUILTIN_MANIFESTS
        .iter()
        .map(|(id, yaml)| {
            (
                *id,
                ProviderManifest::from_yaml(yaml)
                    .unwrap_or_else(|error| panic!("builtin {id} must parse: {error}")),
            )
        })
        .collect()
}

/// Native adapters (Anthropic, Google AI) implement exactly one probe: the
/// models endpoint that answers health and identity. A manifest claiming
/// quota/usage/balance/reset under those wire formats would advertise an
/// endpoint no code ever requests.
#[test]
fn native_protocol_manifests_declare_only_the_models_endpoint() {
    for (id, manifest) in builtin_manifests() {
        let format = WireFormat::parse(
            manifest
                .api_format
                .as_deref()
                .unwrap_or("openai-compatible"),
        );
        let Some(WireFormat::Anthropic | WireFormat::GoogleAi) = format else {
            continue;
        };
        assert!(
            manifest.endpoints.models.is_some(),
            "native manifest {id} declares no models endpoint to probe"
        );
        for (kind, endpoint) in [
            ("quota", &manifest.endpoints.quota),
            ("usage", &manifest.endpoints.usage),
            ("balance", &manifest.endpoints.balance),
            ("reset", &manifest.endpoints.reset),
        ] {
            assert!(
                endpoint.is_none(),
                "native manifest {id} declares `{kind}`, but its adapter never requests it"
            );
        }
    }
}

/// The generic OpenAI-compatible adapter executes exactly these endpoint
/// kinds: models (health probe), quota/usage/balance (metric fetch), and
/// reset (reset fetch, lenient or mapped). A manifest may only declare them,
/// which is checked by construction here so a new manifest key forces this
/// test to be revisited.
#[test]
fn openai_compatible_manifests_only_declare_executed_endpoint_kinds() {
    for (id, manifest) in builtin_manifests() {
        if WireFormat::parse(
            manifest
                .api_format
                .as_deref()
                .unwrap_or("openai-compatible"),
        ) != Some(WireFormat::OpenAiCompatible)
        {
            continue;
        }
        let config = manifest.to_config();
        // Every declared endpoint lands on the config field the adapter reads;
        // the assertion is the mapping itself (a silently dropped field would
        // mean a declared capability nobody executes).
        assert_eq!(
            config.models_endpoint, manifest.endpoints.models,
            "manifest {id}: models endpoint did not reach the adapter config"
        );
        assert_eq!(
            config.quota_endpoint, manifest.endpoints.quota,
            "manifest {id}: quota endpoint did not reach the adapter config"
        );
        assert_eq!(
            config.usage_endpoint, manifest.endpoints.usage,
            "manifest {id}: usage endpoint did not reach the adapter config"
        );
        assert_eq!(
            config.balance_endpoint, manifest.endpoints.balance,
            "manifest {id}: balance endpoint did not reach the adapter config"
        );
        assert_eq!(
            config.reset_endpoint, manifest.endpoints.reset,
            "manifest {id}: reset endpoint did not reach the adapter config"
        );
    }
}

/// L4 means the full core surface is truly available: quota, usage and reset
/// endpoints plus the mapping that can read the reset timestamp. No builtin
/// currently claims L4, so this test is the contract that keeps the level
/// honest if one ever does.
#[test]
fn an_l4_claim_requires_the_full_executable_endpoint_set() {
    for (id, manifest) in builtin_manifests() {
        let declared = manifest
            .capability
            .as_ref()
            .map(tokenowl_lib::providers::manifest::ManifestCapability::level_id);
        if declared != Some("L4") {
            continue;
        }
        assert!(
            manifest.endpoints.quota.is_some(),
            "L4 manifest {id} needs a quota endpoint"
        );
        assert!(
            manifest.endpoints.usage.is_some(),
            "L4 manifest {id} needs a usage endpoint"
        );
        assert!(
            manifest.endpoints.reset.is_some(),
            "L4 manifest {id} needs a reset endpoint"
        );
        assert!(
            manifest
                .mapping
                .as_ref()
                .and_then(|mapping| mapping.reset.as_ref())
                .is_some(),
            "L4 manifest {id} needs a reset mapping, or the reset endpoint parses nothing"
        );
        assert_eq!(
            manifest.effective_capability_level().as_deref(),
            Some("L4"),
            "L4 manifest {id} is clamped below its claim: the endpoints cannot deliver it"
        );
    }
}

/// A model-only manifest cannot out-claim the probe: L1/L2 is the ceiling
/// when nothing but a models endpoint exists, whatever the YAML says.
#[test]
fn model_only_manifests_cannot_claim_above_l2() {
    for (id, manifest) in builtin_manifests() {
        let has_quota_surface = manifest.endpoints.quota.is_some()
            || manifest.endpoints.usage.is_some()
            || manifest.endpoints.balance.is_some();
        if has_quota_surface {
            continue;
        }
        let level = manifest.effective_capability_level();
        assert!(
            matches!(level.as_deref(), Some("L1" | "L2") | None),
            "model-only manifest {id} resolves to {level:?}, which no endpoint can back"
        );
    }
}

//! The README's provider table is generated, never hand-maintained: a drift
//! between the shipped manifests and the documented capabilities fails this
//! test. Regenerate with `TOKENOWL_UPDATE_README=1 cargo test --test readme_sync`
//! and review the diff.

use std::path::PathBuf;

use tokenowl_lib::providers::manifest::ProviderManifest;

const BEGIN: &str = "<!-- providers:begin -->";
const END: &str = "<!-- providers:end -->";

fn readme_path() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../README.md"))
}

/// The generated provider section, in manifest order.
fn generated_section() -> String {
    let mut rows = String::new();
    let mut levels: Vec<(String, usize)> = Vec::new();
    for (_id, yaml) in tokenowl_lib::providers::builtin::BUILTIN_MANIFESTS {
        let manifest = ProviderManifest::from_yaml(yaml).expect("embedded manifest parses");
        let level = manifest
            .effective_capability_level()
            .unwrap_or_else(|| "L1".to_owned());
        let format = manifest
            .api_format
            .as_deref()
            .unwrap_or("openai-compatible");
        let mut endpoints: Vec<String> = Vec::new();
        if manifest.endpoints.models.is_some() {
            endpoints.push("模型".to_owned());
        }
        if manifest.endpoints.quota.is_some() {
            endpoints.push("额度".to_owned());
        }
        if manifest.endpoints.usage.is_some() {
            endpoints.push("用量".to_owned());
        }
        if manifest.endpoints.balance.is_some() {
            endpoints.push("余额".to_owned());
        }
        if manifest.endpoints.reset.is_some() {
            endpoints.push("重置".to_owned());
        }
        let verified = manifest
            .capability
            .as_ref()
            .and_then(|capability| capability.last_verified_at.clone())
            .unwrap_or_else(|| "未声明".to_owned());
        rows.push_str(&format!(
            "| {} ({}) | `{}` | {} | {} | {} |\n",
            manifest.name,
            manifest.id,
            format,
            level,
            if endpoints.is_empty() {
                "—".to_owned()
            } else {
                endpoints.join("、")
            },
            verified,
        ));
        match levels.iter_mut().find(|(name, _)| *name == level) {
            Some((_, count)) => *count += 1,
            None => levels.push((level, 1)),
        }
    }
    levels.sort();
    let summary = levels
        .iter()
        .map(|(level, count)| format!("{level}×{count}"))
        .collect::<Vec<_>>()
        .join("，");
    format!(
        "{BEGIN}\n{} 个内置清单横跨三种请求协议，按各家 API 实际暴露的能力分级 \
        （本表由 `providers/builtin/*.yaml` 生成；表格与清单漂移会让 `cargo test` \
         失败。等级分布：{summary}。能力等级：L1 已接入无已验证额度；L2 连接/模型目录探测；L3 用量或余额；\
         L4 额度 + 用量 + 重置全量且经测试验证。「最近验证」未声明的供应商表示尚未记录\
         一次端到端核验，不代表不可用）：\n\n\
         | 供应商 | 请求协议 | 等级 | 端点 | 最近验证 |\n\
         |---|---|---|---|---|\n\
         {rows}{END}",
        tokenowl_lib::providers::builtin::BUILTIN_MANIFESTS.len(),
        summary = summary,
    )
}

#[test]
fn readme_provider_table_matches_the_manifests() {
    let path = readme_path();
    let content = std::fs::read_to_string(&path).expect("README.md is readable");
    let (Some(begin), Some(end)) = (content.find(BEGIN), content.find(END)) else {
        panic!("README.md must carry the {BEGIN} / {END} markers around the provider table");
    };
    let expected = generated_section();
    if std::env::var("TOKENOWL_UPDATE_README").as_deref() == Ok("1") {
        let updated = format!(
            "{}{}{}",
            &content[..begin],
            expected,
            &content[end + END.len()..],
        );
        std::fs::write(&path, updated).expect("README.md is writable");
        println!("README provider table regenerated");
        return;
    }
    let current = &content[begin..end + END.len()];
    assert_eq!(
        current, expected,
        "README provider table drifted from the manifests; regenerate with \
         TOKENOWL_UPDATE_README=1 cargo test --test readme_sync"
    );
}

//! Proxies (AE File ▸ Set Proxy, Create Proxy): a stand-in file for a footage
//! item or a comp, used instead of it where proxies are on (previews by
//! default) and drawn at the item's size. The proxy is an `Asset` prim named
//! `proxy` under the item (its file is found like any footage file);
//! `proxy:enabled` on the item says whether it is used.

use std::path::Path;

use bae_model::asset::AssetFile;
use bae_model::{Command, CommandError, PrimPath, PrimType, Stage, Transaction, Value};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::import::{ImportFile, footage_attr_allowed};
use crate::util::{create, ensure_unlocked, fail, set};

pub use bae_model::asset::{PROXY_ENABLED as ENABLED, PROXY_PRIM as PRIM, proxy_of};

/// Whether `item` has a proxy and it is on (the Project panel's filled square).
pub fn proxy_enabled(stage: &Stage, item: &PrimPath) -> bool {
    bae_model::asset::proxy_in_use(stage, item).is_some()
}

/// Footage items and comps can have a proxy.
fn proxiable(stage: &Stage, item: &PrimPath) -> bool {
    stage.prim(item).is_some_and(|v| {
        v.is_a(&PrimType::COMP)
            || (v.is_a(&PrimType::ASSET)
                && !matches!(v.value("kind").and_then(Value::as_str), Some("solid")))
    })
}

pub struct SetProxy;

#[derive(Deserialize, JsonSchema)]
pub struct SetProxyParams {
    /// Footage items or comps.
    pub items: Vec<PrimPath>,
    /// The proxy file with its probed attributes (as for `item.import`);
    /// omitted: File ▸ Set Proxy ▸ None (the proxy is removed).
    #[serde(default)]
    pub file: Option<ImportFile>,
}

impl Command for SetProxy {
    type Params = SetProxyParams;
    type Output = ();
    const ID: &'static str = "item.set_proxy";
    const FILESYSTEM: bool = true;
    const TITLE: &'static str = "Set Proxy";
    const DESCRIPTION: &'static str = "File > Set Proxy > File... / None: give footage items or comps a proxy file (probed attributes as for item.import) and turn it on, or remove their proxies. One undo step.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        if p.items.is_empty() {
            return Err(fail("no items given"));
        }
        ensure_unlocked(tx.stage(), &p.items)?;
        let file = match &p.file {
            Some(f) => {
                if let Some((bad, _)) = f.attrs.iter().find(|(k, _)| !footage_attr_allowed(k)) {
                    return Err(fail(format!("{bad} is not a footage attribute")));
                }
                Some(
                    AssetFile::from_path(Path::new(&f.path), None, false)
                        .map_err(|e| fail(format!("cannot use {} as a proxy: {e}", f.path)))?,
                )
            }
            None => None,
        };
        for item in &p.items {
            if !proxiable(tx.stage(), item) {
                return Err(fail(format!("{item} cannot have a proxy")));
            }
            if let Some(old) = proxy_of(tx.stage(), item) {
                crate::layer::DeleteItems
                    .run(tx, crate::layer::DeleteItemsParams { paths: vec![old] })?;
            }
            let (Some(f), Some(file)) = (&p.file, &file) else {
                set(tx, item, ENABLED, Value::Bool(false))?;
                continue;
            };
            let proxy = item
                .child(PRIM)
                .map_err(|e| fail(format!("proxy path: {e}")))?;
            create(tx, &proxy, PrimType::ASSET, None)?;
            let name = Path::new(&f.path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            set(tx, &proxy, "name", Value::String(name))?;
            if !f.attrs.contains_key("kind") {
                set(tx, &proxy, "kind", Value::Enum("footage".into()))?;
            }
            for (attr, value) in &f.attrs {
                set(tx, &proxy, attr, value.clone())?;
            }
            for op in file.to_ops(&proxy) {
                tx.apply(op)?;
            }
            set(tx, item, ENABLED, Value::Bool(true))?;
        }
        Ok(())
    }
}

pub struct SetProxyEnabled;

#[derive(Deserialize, JsonSchema)]
pub struct SetProxyEnabledParams {
    pub items: Vec<PrimPath>,
    /// On or off; omitted toggles each item (the Project panel's square).
    #[serde(default)]
    pub enabled: Option<bool>,
}

impl Command for SetProxyEnabled {
    type Params = SetProxyEnabledParams;
    type Output = ();
    const ID: &'static str = "item.set_proxy_enabled";
    const TITLE: &'static str = "Use Proxy";
    const DESCRIPTION: &'static str = "Turn the proxies of footage items or comps on or off (AE's proxy square left of the item name in the Project panel); items without a proxy are left alone.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        for item in &p.items {
            if proxy_of(tx.stage(), item).is_none() {
                continue;
            }
            let on = p
                .enabled
                .unwrap_or_else(|| !proxy_enabled(tx.stage(), item));
            set(tx, item, ENABLED, Value::Bool(on))?;
        }
        Ok(())
    }
}

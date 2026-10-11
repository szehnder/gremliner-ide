//! The app's asset source: gpui-kit's default component icons plus the extra Lucide icons we use.
//! Register it with `application().with_assets(AppAssets)`.

use std::borrow::Cow;

use gpui_kit::assets::{Assets, icon_assets};
use gpui_kit::{AssetSource, Result, SharedString};

icon_assets!(
    ExtraIcons,
    [
        Database, Waypoints, Keyboard, Plug, Command, Sparkles, Unplug
    ]
);

/// Our extras, then gpui-kit's default icons. Order matters: `Assets` reports an unknown path as an
/// error rather than `None`, so it has to be the last source tried.
pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        match ExtraIcons.load(path)? {
            Some(bytes) => Ok(Some(bytes)),
            None => Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut all = Assets.list(path)?;
        all.extend(ExtraIcons.list(path)?);
        all.sort();
        all.dedup();
        Ok(all)
    }
}

/// Icons from the extra set, as gpui-kit's shared icon names.
pub mod icons {
    pub use gpui_kit::assets::IconName;
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use gpui_kit::AssetSource as _;

    use super::{AppAssets, icons::IconName};

    #[test]
    fn default_and_extra_icons_load() {
        for name in [
            IconName::Plus,
            IconName::Waypoints,
            IconName::Database,
            IconName::Command,
        ] {
            let path = name.path();
            assert!(AppAssets.load(&path).unwrap().is_some(), "{path} missing");
        }
    }
}

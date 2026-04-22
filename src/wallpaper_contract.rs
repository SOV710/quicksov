// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

include!(concat!(env!("OUT_DIR"), "/wallpaper_contract.rs"));

pub fn default_wallpaper_decode_backend_order() -> Vec<String> {
    WALLPAPER_DEFAULT_DECODE_BACKEND_ORDER
        .iter()
        .map(|backend| (*backend).to_string())
        .collect()
}

pub fn normalize_wallpaper_decode_backend_order(backends: &[String]) -> (Vec<String>, Vec<String>) {
    let mut normalized = Vec::new();
    let mut unsupported = Vec::new();

    for backend in backends {
        let backend = backend.trim().to_ascii_lowercase();
        if backend.is_empty() {
            continue;
        }
        if !WALLPAPER_DECODE_BACKEND_CATALOG.contains(&backend.as_str()) {
            unsupported.push(backend);
            continue;
        }
        if !normalized.contains(&backend) {
            normalized.push(backend);
        }
    }

    if !normalized
        .iter()
        .any(|backend| backend == WALLPAPER_SOFTWARE_DECODE_BACKEND)
    {
        normalized.push(WALLPAPER_SOFTWARE_DECODE_BACKEND.to_string());
    }

    (normalized, unsupported)
}

#[cfg(test)]
mod tests {
    use super::{
        default_wallpaper_decode_backend_order, normalize_wallpaper_decode_backend_order,
        WALLPAPER_DECODE_BACKEND_CATALOG, WALLPAPER_SOFTWARE_DECODE_BACKEND,
    };

    #[test]
    fn default_decode_order_stays_inside_catalog() {
        let default_order = default_wallpaper_decode_backend_order();
        assert!(default_order
            .iter()
            .all(|backend| WALLPAPER_DECODE_BACKEND_CATALOG.contains(&backend.as_str())));
        assert_eq!(
            default_order.last().map(String::as_str),
            Some(WALLPAPER_SOFTWARE_DECODE_BACKEND)
        );
    }

    #[test]
    fn normalize_decode_order_filters_unknown_and_appends_software() {
        let (normalized, unsupported) = normalize_wallpaper_decode_backend_order(&[
            "CUDA".to_string(),
            "bogus".to_string(),
            "vulkan".to_string(),
            "cuda".to_string(),
        ]);

        assert_eq!(unsupported, vec!["bogus"]);
        assert_eq!(
            normalized,
            vec![
                "cuda".to_string(),
                "vulkan".to_string(),
                WALLPAPER_SOFTWARE_DECODE_BACKEND.to_string()
            ]
        );
    }
}

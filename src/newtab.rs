use crate::storage::{escape_html, valid_web_url, BrowserData};
const START_PAGE: &str = include_str!("../ui/start.html");
pub fn valid_background(value: &str) -> bool {
    if value.is_empty() {
        return true;
    }
    if value.len() > 900_000 {
        return false;
    }
    if let Some(data) = value.strip_prefix("data:image/jpeg;base64,") {
        use base64::Engine;
        return base64::engine::general_purpose::STANDARD
            .decode(data)
            .is_ok_and(|bytes| bytes.starts_with(&[0xff, 0xd8, 0xff]));
    }
    value.len() <= 4096
        && value.starts_with("https://")
        && crate::security::safe_top_level_navigation(value)
}
pub fn render(data: &BrowserData, private_mode: bool) -> String {
    render_localized(data, private_mode, data.browser_settings.language)
}

fn render_localized(
    data: &BrowserData,
    private_mode: bool,
    language: crate::storage::Language,
) -> String {
    let defaults = [
        ("https://www.google.com/", "Google"),
        ("https://mail.google.com/", language.pick("Почта", "Mail")),
        ("https://www.youtube.com/", "YouTube"),
        (
            language.pick("https://ru.wikipedia.org/", "https://en.wikipedia.org/"),
            language.pick("Википедия", "Wikipedia"),
        ),
    ];
    let entries: Vec<(&str, &str)> = if data.bookmarks.is_empty() {
        defaults.to_vec()
    } else {
        data.bookmarks
            .iter()
            .take(usize::from(data.customization.validated().shortcut_count))
            .map(|b| (b.url.as_str(), b.title.as_str()))
            .collect()
    };
    let shortcuts=entries.into_iter().filter(|(url,_)|valid_web_url(url)).map(|(url,title)|{
        let title=if title.is_empty(){url}else{title};let initial=title.chars().next().unwrap_or('L');
        let icon=data.custom_icons.get(&format!("site:{url}")).filter(|value|crate::icons::png_bytes(value).is_some()).map(|value|format!("<img alt=\"\" width=\"32\" height=\"32\" src=\"{}\">",escape_html(value))).unwrap_or_else(||escape_html(&initial.to_string()));
        format!("<a class=\"shortcut\" href=\"{}\" title=\"{}\"><span class=\"shortcut-icon\">{}</span><span class=\"shortcut-label\">{}</span></a>",escape_html(url),escape_html(title),icon,escape_html(title))
    }).collect::<String>();
    let c = data.customization.validated();
    let mut extra = format!(".wallpaper{{object-fit:{};filter:blur({}px)}}{}{}{}",
        if c.wallpaper_contain { "contain" } else { "cover" }, c.wallpaper_blur,
        if c.show_shortcuts { "" } else { ".workspace{display:none!important}" },
        if c.show_services { "" } else { ".header-actions{display:none!important}" },
        match c.home_layout { 1 => "main{grid-template-columns:1fr;max-width:850px;text-align:center}.workspace{text-align:left}.lead{margin-inline:auto}", 2 => ".workspace,.eyebrow,.note{display:none!important}main{grid-template-columns:1fr;max-width:850px}", _ => "" });
    if data.background_style.mode > 0 {
        extra.push_str(&format!(
            "body{{{}}}main,header,footer{{background:var(--bg);border-radius:20px}}",
            data.background_style.css()
        ));
    }
    let mut home = data
        .home
        .clone()
        .unwrap_or_else(|| crate::home::Dashboard::from_legacy(data));
    if data.home.is_none() {
        home.subtitle_text = language
            .pick(
                "Начните с вопроса. Продолжите открытием.",
                "Start with a question. See where it takes you.",
            )
            .into();
    }
    let bookmarks: Vec<_> = data
        .bookmarks
        .iter()
        .take(200)
        .map(|b| crate::home::Tile {
            title: b.title.clone(),
            url: b.url.clone(),
        })
        .collect();
    let suggestions: Vec<_> = if private_mode {
        Vec::new()
    } else {
        crate::navigation::suggestion_candidates(data)
            .filter(|entry| entry.title.len() + entry.url.len() <= 512)
            .take(80)
            .collect()
    };
    let template = START_PAGE.replace("/* HOME_SCRIPT */", include_str!("../ui/home-editor.js"));
    crate::i18n::template(&template, language)
        .replace("__LOCALE__", language.pick("ru-RU", "en-US"))
        .replace(
            "__HOME_SUGGESTIONS__",
            &crate::home::script_json(&suggestions),
        )
        .replace("__HOME_JSON__", &crate::home::script_json(&home))
        .replace("__BOOKMARKS_JSON__", &crate::home::script_json(&bookmarks))
        .replace(
            "__HOME_ICONS__",
            &crate::home::script_json(&data.custom_icons),
        )
        .replace("__EXTRA_STYLE__", &extra)
        .replace("__HOUR12__", if c.clock_24 { "false" } else { "true" })
        .replace(
            "__CUSTOM_STYLE__",
            &format!(
                "body{{--wallpaper-dim:{};}}{}{}{}",
                f64::from(data.customization.wallpaper_dim.min(90)) / 100.0,
                if data.customization.animations {
                    ""
                } else {
                    "*,*::before,*::after{animation:none!important;transition:none!important}"
                },
                if data.customization.show_clock {
                    ""
                } else {
                    ".clock,.date{display:none}"
                },
                if data.customization.show_greeting {
                    ""
                } else {
                    "#greeting,.lead{display:none}"
                }
            ),
        )
        .replace(
            "<!-- BACKGROUND -->",
            &if data.background_style.mode == 0
                && !data.background.is_empty()
                && valid_background(&data.background)
            {
                format!(
                    "<img class=\"wallpaper\" alt=\"\" referrerpolicy=\"no-referrer\" src=\"{}\">",
                    escape_html(&data.background)
                )
            } else {
                String::new()
            },
        )
        .replace(
            "__SEARCH_NAME__",
            &crate::i18n::template(data.search_engine.name(), language),
        )
        .replace("__SEARCH_URL__", data.search_engine.prefix())
        .replace("<!-- SHORTCUTS -->", &shortcuts)
        .replace(
            "__PROFILE_NOTE__",
            if private_mode {
                language.pick(
                    "Приватное окно · Вход на сайты не сохраняется после закрытия",
                    "Private window · Site sessions end when you close the window",
                )
            } else {
                language.pick(
                    "LiteRust · Сессии сайтов хранятся в вашем профиле",
                    "LiteRust · Site sessions are stored in your profile",
                )
            },
        )
}

pub fn render_with_profile(
    data: &BrowserData,
    private_mode: bool,
    profile: Option<&crate::local_profiles::Profile>,
) -> String {
    render_with_profile_localized(data, private_mode, profile, data.browser_settings.language)
}

pub fn render_with_profile_localized(
    data: &BrowserData,
    private_mode: bool,
    profile: Option<&crate::local_profiles::Profile>,
    language: crate::storage::Language,
) -> String {
    let html = render_localized(data, private_mode, language);
    let avatar = profile
        .map(|p| {
            if !private_mode && crate::icons::png_bytes(&p.avatar).is_some() {
                format!("<img alt=\"\" src=\"{}\">", escape_html(&p.avatar))
            } else {
                escape_html(&p.name.chars().next().unwrap_or('L').to_string())
            }
        })
        .unwrap_or_else(|| "L".into());
    html.replace("<!-- LOCAL_PROFILE_AVATAR -->L", &avatar)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn english_template_preserves_user_text_and_private_mode_omits_history() {
        let mut data = BrowserData::default();
        data.browser_settings.language = crate::storage::Language::En;
        data.toggle_bookmark("https://example.org/", "Настройки");
        data.record_visit("https://history.example/", "History entry");
        let normal = render(&data, false);
        assert!(normal.contains("lang=\"en\""));
        assert!(normal.contains("<title>New tab</title>"));
        assert!(normal.contains("Настройки"));
        assert!(normal.contains("https://history.example/"));
        assert!(!render(&data, true).contains("https://history.example/"));
    }

    #[test]
    fn safe_shortcuts_and_profile_note() {
        let mut data = BrowserData::default();
        data.toggle_bookmark("https://example.com/", "<img src=x onerror=alert(1)>");
        let html = render(&data, false);
        assert!(!html.contains("<img src=x"));
        assert!(html.contains("&lt;img"));
        assert!(html.contains("https://example.com/"));
        assert!(!html.contains("<!-- SHORTCUTS -->"));
        assert!(render(&data, true).contains("Вход на сайты не сохраняется"));
        for i in 0..20 {
            data.toggle_bookmark(&format!("https://example.com/{i}"), "Test");
        }
        assert_eq!(
            render(&data, false).matches("class=\"shortcut\"").count(),
            10
        );
    }
    #[test]
    fn backgrounds_reject_active_content_and_escape_attributes() {
        for value in [
            "javascript:alert(1)",
            "file:///C:/secret",
            "data:image/svg+xml,<svg/>",
            "http://example.org/a.png",
            "https://user:pass@example.org/a",
        ] {
            assert!(!valid_background(value));
        }
        assert!(valid_background(""));
        assert!(!valid_background(&"x".repeat(900_001)));
        let data = BrowserData {
            background: "https://example.org/image?a=1&b=2".into(),
            ..BrowserData::default()
        };
        assert!(valid_background(&data.background));
        let html = render(&data, false);
        assert!(html.contains("a=1&amp;b=2"));
        assert!(html.contains("referrerpolicy=\"no-referrer\""));
        let old: BrowserData = serde_json::from_str("{}").unwrap();
        assert!(old.background.is_empty());
    }
    #[test]
    fn colored_background_overrides_image_and_preserves_account_entry() {
        let mut data = BrowserData {
            background: "https://example.org/a.jpg".into(),
            ..BrowserData::default()
        };
        data.background_style = crate::storage::BackgroundStyle {
            mode: 2,
            color_a: 0x123456,
            color_b: 0xabcdef,
            angle: 90,
        };
        let html = render(&data, false);
        assert!(html.contains("background:linear-gradient(90deg,#123456,#abcdef)!important"));
        assert!(!html.contains("src=\"https://example.org/a.jpg"));
        assert!(html.contains("href=\"literust://accounts\" aria-label=\"Локальный профиль\""));
        data.background_style.mode = 0;
        assert!(render(&data, false).contains("src=\"https://example.org/a.jpg"));
    }
    #[test]
    fn selected_provider_is_rendered() {
        let data = BrowserData {
            search_engine: crate::navigation::SearchEngine::Yandex,
            ..BrowserData::default()
        };
        let html = render(&data, false);
        assert!(html.contains("Поиск · Яндекс"));
        assert!(html.contains("homeSend({action:'navigate',value:q})"));
        assert!(html.contains("Поиск · Яндекс"));
        assert!(!html.contains("__SEARCH_"));
    }
    #[test]
    fn default_services_and_signin() {
        let html = render(&BrowserData::default(), false);
        assert_eq!(html.matches("class=\"shortcut\"").count(), 4);
        assert!(html.contains("class=\"sign-in profile-avatar\" href=\"literust://accounts\""));
        assert!(html.contains("Поиск · Google"));
    }
}

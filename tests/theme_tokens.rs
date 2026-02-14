use textual::style::{Color, parse_color_like};

fn expect_token(name: &str, expected: Color) {
    let token = format!("${name}");
    let got = parse_color_like(&token).unwrap_or_else(|| panic!("missing token {token}"));
    assert_eq!(got, expected, "token {token} mismatch");
}

#[test]
fn textual_dark_button_related_tokens_match_python_values() {
    expect_token("surface-lighten-1", Color::parse("#2D2D2D").unwrap());
    expect_token("surface-darken-1", Color::parse("#0D0D0D").unwrap());
    expect_token("primary-lighten-3", Color::parse("#6DB2FF").unwrap());
    expect_token("primary-darken-3", Color::parse("#004295").unwrap());
    expect_token("primary-darken-2", Color::parse("#0053AA").unwrap());
    expect_token("primary-muted", Color::parse("#0C304C").unwrap());
    expect_token("success-lighten-2", Color::parse("#7AE998").unwrap());
    expect_token("success-darken-3", Color::parse("#008139").unwrap());
    expect_token("success-darken-2", Color::parse("#18954B").unwrap());
    expect_token("success-muted", Color::parse("#24452E").unwrap());
    expect_token("warning-lighten-2", Color::parse("#FFCF56").unwrap());
    expect_token("warning-darken-3", Color::parse("#B86B00").unwrap());
    expect_token("warning-darken-2", Color::parse("#CF7E00").unwrap());
    expect_token("warning-muted", Color::parse("#593E19").unwrap());
    expect_token("error-lighten-2", Color::parse("#E76580").unwrap());
    expect_token("error-darken-3", Color::parse("#780028").unwrap());
    expect_token("error-darken-2", Color::parse("#8D0638").unwrap());
    expect_token("error-darken-1", Color::parse("#A32549").unwrap());
    expect_token("error-muted", Color::parse("#441E27").unwrap());
}

#[test]
fn textual_dark_semantic_text_tokens_are_available() {
    expect_token("text-primary", Color::parse("#57A5E2").unwrap());
    expect_token("text-secondary", Color::parse("#5684A5").unwrap());
    expect_token("text-warning", Color::parse("#FFC473").unwrap());
    expect_token("text-error", Color::parse("#D17E92").unwrap());
    expect_token("text-success", Color::parse("#8AD4A1").unwrap());
    expect_token("text-accent", Color::parse("#FFC473").unwrap());
    expect_token("foreground-muted", Color::parse("#E0E0E099").unwrap());
    expect_token("foreground-disabled", Color::parse("#E0E0E060").unwrap());
}

#[test]
fn textual_dark_markdown_heading_background_tokens_are_available() {
    let transparent = Color::rgba(0, 0, 0, 0);
    expect_token("markdown-h1-background", transparent);
    expect_token("markdown-h2-background", transparent);
    expect_token("markdown-h3-background", transparent);
    expect_token("markdown-h4-background", transparent);
    expect_token("markdown-h5-background", transparent);
    expect_token("markdown-h6-background", transparent);
}

#[test]
fn textual_dark_auto_like_tokens_use_alpha() {
    // Python textual-dark: text=auto 87%, button-color-foreground=auto 87%.
    // In textual-rs this is represented as white with alpha 0.87, then flattened
    // against the current background during style application.
    let expected_auto = Color::rgba(255, 255, 255, 222);
    expect_token("text", expected_auto);
    expect_token("button-color-foreground", expected_auto);
}

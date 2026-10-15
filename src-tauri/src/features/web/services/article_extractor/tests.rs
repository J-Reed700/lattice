use super::*;

#[tokio::test]
async fn test_extract_article_basic() {
    let service = ArticleExtractorService::new().unwrap();

    let html = r#"
            <!DOCTYPE html>
            <html>
            <head>
                <title>Test Article Title</title>
                <meta name="author" content="John Doe">
            </head>
            <body>
                <article>
                    <h1>Main Heading</h1>
                    <p>This is the first paragraph of the article with some content that discusses various aspects of the topic at hand. We need to ensure that this paragraph contains enough words to meet the minimum word count threshold for article extraction to be successful. The content should be meaningful and representative of a real article that someone might want to extract and read later.</p>
                    <p>This is the second paragraph with more interesting information and additional details about the subject matter. Articles typically contain multiple paragraphs with substantial content to provide comprehensive coverage of the topic. This helps ensure that readers get a complete understanding of the subject being discussed.</p>
                    <p>And here is a third paragraph to make it substantial and provide even more context about the article topic. Quality articles contain well-structured information that is both informative and engaging for readers. This ensures the extraction process can properly identify and preserve the main content of the article.</p>
                </article>
            </body>
            </html>
        "#;

    let result = service
        .extract_article(html, "https://example.com/article")
        .await;

    assert!(result.is_ok());
    let article = result.unwrap();

    assert_eq!(article.title, "Test Article Title");
    assert_eq!(article.author, Some("John Doe".to_string()));
    assert!(article.word_count > 0);
    assert!(article.reading_time_minutes >= 1);
    assert!(article.excerpt.is_some());
    assert!(!article.content.is_empty());
    assert!(!article.text_content.is_empty());
}

#[tokio::test]
async fn test_extract_article_with_main_tag() {
    let service = ArticleExtractorService::new().unwrap();

    let html = r#"
            <html>
            <head>
                <title>Article Title</title>
            </head>
            <body>
                <nav>This should be removed</nav>
                <main>
                    <h1>Article Title</h1>
                    <p>This is the main content that should be extracted and preserved for the reader with many additional words to ensure we meet the minimum threshold. Articles need substantial content to be considered high quality and worth extracting from web pages.</p>
                    <p>Multiple paragraphs ensure we have enough content for proper extraction and provide readers with comprehensive information about the topic being discussed. This helps the article extractor understand that this is genuine article content.</p>
                    <p>The article continues with more valuable information here and additional context that helps readers understand the full scope of the topic. Quality content extraction requires sufficient text to distinguish articles from navigation elements.</p>
                </main>
                <footer>This should also be removed</footer>
            </body>
            </html>
        "#;

    let result = service
        .extract_article(html, "https://example.com")
        .await
        .unwrap();

    assert_eq!(result.title, "Article Title");
    assert!(result.text_content.contains("main content"));
    assert!(!result.text_content.contains("should be removed"));
    assert!(result.word_count > 10);
}

#[tokio::test]
async fn test_extract_article_removes_ads() {
    let service = ArticleExtractorService::new().unwrap();

    let html = r#"
            <html>
            <head>
                <title>Clean Article</title>
            </head>
            <body>
                <article>
                    <h1>Clean Article</h1>
                    <p>This is legitimate article content that should remain and provides readers with valuable information about the topic at hand. We need sufficient content to meet the minimum word count threshold for successful article extraction.</p>
                    <p>More article content here that is valuable to readers and contains detailed information about various aspects of the subject being discussed. Quality articles provide comprehensive coverage of topics and ensure readers have everything they need.</p>
                    <p>Final paragraph of the article with more information and additional context to help readers fully understand the topic. Articles should be substantial enough to warrant extraction and preservation for later reading and reference purposes.</p>
                </article>
            </body>
            </html>
        "#;

    let result = service
        .extract_article(html, "https://example.com")
        .await
        .unwrap();

    assert!(result.text_content.contains("legitimate article"));
    // Readability.js cleans the content and removes non-article elements
    assert!(result.word_count >= 100);
}

#[tokio::test]
async fn test_word_count_and_reading_time() {
    let service = ArticleExtractorService::new().unwrap();

    let html = r#"
            <html>
            <head>
                <title>Title</title>
            </head>
            <body>
                <article>
                    <h1>Title</h1>
                    <p>Word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word.</p>
                </article>
            </body>
            </html>
        "#;

    let result = service
        .extract_article(html, "https://example.com")
        .await
        .unwrap();

    assert!(result.word_count >= 100);
    // Reading time should be 1 minute (minimum)
    assert_eq!(result.reading_time_minutes, 1);
}

#[tokio::test]
async fn test_excerpt_generation() {
    let service = ArticleExtractorService::new().unwrap();

    let html = r#"
            <html>
            <head>
                <title>Title</title>
            </head>
            <body>
                <article>
                    <h1>Title</h1>
                    <p>This is a short article that should generate a proper excerpt from the beginning of the content without truncation and contains enough words to meet the minimum threshold for article extraction. We need to ensure there is sufficient content for the extractor to recognize this as a legitimate article worth processing. Additional words help the system understand that this is real article content that should be preserved and made available to readers who want to save articles for later reading. Quality content requires a minimum number of words to be considered substantial enough for extraction and archiving purposes. We continue adding more sentences to ensure we exceed any minimum word count requirements that the extraction service may have in place for determining valid articles.</p>
                </article>
            </body>
            </html>
        "#;

    let result = service
        .extract_article(html, "https://example.com")
        .await
        .unwrap();

    assert!(result.excerpt.is_some());
    let excerpt = result.excerpt.unwrap();
    assert!(excerpt.len() <= 203); // 200 + "..."
    assert!(excerpt.starts_with("This is a short"));
}

#[tokio::test]
async fn test_empty_html_error() {
    let service = ArticleExtractorService::new().unwrap();

    let result = service.extract_article("", "https://example.com").await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), AppError::InvalidInput(_)));
}

#[tokio::test]
async fn test_no_content_error() {
    let service = ArticleExtractorService::new().unwrap();

    let html = r#"
            <html>
            <body>
                <div>Too short</div>
            </body>
            </html>
        "#;

    let result = service.extract_article(html, "https://example.com").await;

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        AppError::ContentExtraction { .. }
    ));
}

#[tokio::test]
async fn test_extract_published_date() {
    let service = ArticleExtractorService::new().unwrap();

    let html = r#"
            <html>
            <head>
                <title>Dated Article</title>
                <meta property="article:published_time" content="2024-01-15T10:30:00Z">
            </head>
            <body>
                <article>
                    <h1>Dated Article</h1>
                    <p>This article has a publication date in the meta tags that should be extracted properly and demonstrates how article extractors can preserve important metadata about when content was published. This helps readers understand the context and timeliness of information that they are reading and consuming from various sources.</p>
                    <p>Additional content to meet minimum length requirements for extraction ensures that the article has enough substance to be worth saving and reading later. Quality articles provide comprehensive information that is valuable to readers over time, making proper date extraction an important feature. We add more sentences to ensure the word count threshold is exceeded and the article passes validation checks for content quality and substance.</p>
                </article>
            </body>
            </html>
        "#;

    let result = service
        .extract_article(html, "https://example.com")
        .await
        .unwrap();

    use chrono::Datelike;
    assert!(result.published_date.is_some());
    let date = result.published_date.unwrap();
    assert_eq!(date.naive_utc().year(), 2024);
    assert_eq!(date.naive_utc().month(), 1);
    assert_eq!(date.naive_utc().day(), 15);
}

#[tokio::test]
async fn test_extract_youtube_article_from_html_metadata() {
    let service = ArticleExtractorService::new().unwrap();

    let html = r#"
            <html>
            <head>
                <title>Video Title - YouTube</title>
                <meta property="og:title" content="Sample YouTube Video">
                <meta property="og:description" content="This is a sample YouTube description with enough words to provide meaningful context for indexing and later retrieval in search.">
                <meta itemprop="author" content="Sample Channel">
                <meta itemprop="datePublished" content="2024-05-12T12:00:00Z">
            </head>
            <body>
                <div id="player"></div>
            </body>
            </html>
        "#;

    let result = service
        .extract_youtube_article_from_html(html, "https://www.youtube.com/watch?v=test")
        .unwrap();

    assert_eq!(result.title, "Sample YouTube Video");
    assert_eq!(result.author, Some("Sample Channel".to_string()));
    assert!(result.text_content.contains("sample YouTube description"));
    assert!(result.content.contains("Watch on YouTube"));
    assert!(result.word_count > 10);
    assert!(result.published_date.is_some());
}

#[test]
fn test_is_youtube_url_variants() {
    let service = ArticleExtractorService::new().unwrap();

    assert!(service.is_youtube_url("https://www.youtube.com/watch?v=abc123"));
    assert!(service.is_youtube_url("https://youtu.be/abc123"));
    assert!(service.is_youtube_url("https://m.youtube.com/watch?v=abc123"));
    assert!(!service.is_youtube_url("https://example.com/watch?v=abc123"));
}

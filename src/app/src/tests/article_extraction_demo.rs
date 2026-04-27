#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(deprecated)]

/// Integration test demonstrating improved article extraction with Readability.js
///
/// This test verifies that the upgraded ArticleExtractorService can extract
/// full article content instead of just snippets.
use lattice::infrastructure::services::article_extractor::ArticleExtractorService;
use lattice::infrastructure::services::traits::ArticleExtractorServiceTrait;

#[tokio::test]
async fn test_extract_full_article_content() {
    let service = ArticleExtractorService::new().expect("Failed to create service");

    // Simulated HTML from a typical blog post with navigation, ads, etc.
    let html = r#"
        <!DOCTYPE html>
        <html>
        <head>
            <title>Understanding Rust's Ownership Model - Tech Blog</title>
            <meta name="author" content="Jane Developer">
            <meta property="article:published_time" content="2024-01-15T10:30:00Z">
        </head>
        <body>
            <!-- Navigation (should be removed) -->
            <nav class="site-nav">
                <a href="/">Home</a>
                <a href="/blog">Blog</a>
                <a href="/about">About</a>
            </nav>

            <!-- Sidebar ad (should be removed) -->
            <aside class="sidebar">
                <div class="advertisement">
                    <h3>Buy Our Course!</h3>
                    <p>Learn Rust in 30 days...</p>
                </div>
            </aside>

            <!-- Main article content (should be extracted) -->
            <article class="post-content">
                <h1>Understanding Rust's Ownership Model</h1>

                <p class="byline">By Jane Developer</p>

                <p>Rust's ownership system is one of its most distinctive features. It enables memory safety without garbage collection by enforcing strict rules at compile time.</p>

                <h2>The Three Rules of Ownership</h2>

                <p>Every piece of data in Rust follows three fundamental rules:</p>

                <ol>
                    <li>Each value in Rust has a variable that's called its owner.</li>
                    <li>There can only be one owner at a time.</li>
                    <li>When the owner goes out of scope, the value will be dropped.</li>
                </ol>

                <h2>Moving vs Borrowing</h2>

                <p>In Rust, when you assign a value to another variable, ownership is moved by default. This prevents double-free errors but requires understanding when to borrow instead.</p>

                <pre><code>
let s1 = String::from("hello");
let s2 = s1; // s1 is moved to s2
// println!("{}", s1); // This would error!
                </code></pre>

                <p>Borrowing allows you to reference data without taking ownership. This is done using references (&).</p>

                <h2>Why This Matters</h2>

                <p>The ownership model eliminates entire classes of bugs:</p>

                <ul>
                    <li>Use-after-free errors</li>
                    <li>Double-free errors</li>
                    <li>Data races in concurrent code</li>
                </ul>

                <p>This article continues with several more paragraphs explaining advanced concepts like lifetimes, smart pointers, and interior mutability patterns. The content is substantial and should be fully extracted, not just the first few sentences.</p>

                <p>Understanding these concepts is crucial for writing safe and efficient Rust code. The compiler's borrow checker might seem strict at first, but it prevents bugs that would otherwise only show up at runtime.</p>

                <p>As you become more familiar with Rust's ownership model, you'll find that it becomes second nature. The patterns that initially felt restrictive will start to feel like natural expressions of program correctness.</p>
            </article>

            <!-- Footer (should be removed) -->
            <footer>
                <p>&copy; 2024 Tech Blog. All rights reserved.</p>
                <div class="social-share">Share this article!</div>
            </footer>

            <!-- Comments section (should be removed) -->
            <div class="comments">
                <h3>Comments</h3>
                <p>Great article! - User123</p>
            </div>
        </body>
        </html>
    "#;

    let result = service
        .extract_article(html, "https://techblog.example.com/rust-ownership")
        .await;

    assert!(result.is_ok(), "Article extraction should succeed");

    let article = result.unwrap();

    // Verify metadata extraction
    assert_eq!(
        article.title,
        "Understanding Rust's Ownership Model - Tech Blog"
    );
    assert_eq!(article.author, Some("Jane Developer".to_string()));
    assert!(article.published_date.is_some());

    // Verify content extraction
    assert!(!article.content.is_empty(), "Content should not be empty");
    assert!(
        !article.text_content.is_empty(),
        "Text content should not be empty"
    );

    // Verify FULL content extraction (not just snippets)
    // The old implementation would only extract ~100-500 chars
    // The new implementation should extract the complete article
    assert!(
        article.word_count > 100,
        "Word count should be substantial (got {})",
        article.word_count
    );

    // Verify main content is present
    assert!(
        article.text_content.contains("ownership system"),
        "Should contain main article content"
    );
    assert!(
        article.text_content.contains("Three Rules of Ownership"),
        "Should contain section headings"
    );
    assert!(
        article.text_content.contains("moving"),
        "Should contain key concepts"
    );
    assert!(
        article.text_content.contains("borrow checker"),
        "Should contain content from later paragraphs"
    );

    // Verify unwanted content is removed
    assert!(
        !article.text_content.contains("Buy Our Course"),
        "Should not contain advertisement text"
    );
    assert!(
        !article.text_content.contains("Share this article"),
        "Should not contain footer content"
    );
    assert!(
        !article.text_content.contains("Great article! - User123"),
        "Should not contain comments"
    );

    // Verify reading time calculation
    assert!(
        article.reading_time_minutes >= 1,
        "Reading time should be at least 1 minute"
    );

    // Verify excerpt generation
    assert!(article.excerpt.is_some());
    let excerpt = article.excerpt.unwrap();
    assert!(!excerpt.is_empty(), "Excerpt should not be empty");
    assert!(
        excerpt.len() <= 203,
        "Excerpt should be truncated to ~200 chars"
    );

    println!("\n=== Article Extraction Test Results ===");
    println!("Title: {}", article.title);
    println!("Author: {:?}", article.author);
    println!("Published: {:?}", article.published_date);
    println!("Word count: {}", article.word_count);
    println!("Reading time: {} minutes", article.reading_time_minutes);
    println!("Content length: {} chars", article.content.len());
    println!("Text length: {} chars", article.text_content.len());
    println!("Excerpt: {}", excerpt);
    println!("\n=== Full Text Content (first 500 chars) ===");
    println!(
        "{}...",
        article.text_content.chars().take(500).collect::<String>()
    );
    println!("\n✓ Full article content extracted successfully!");
}

#[tokio::test]
async fn test_extract_article_with_complex_html() {
    let service = ArticleExtractorService::new().expect("Failed to create service");

    // Test with a more complex HTML structure similar to Medium or news sites
    let html = r#"
        <!DOCTYPE html>
        <html>
        <head>
            <title>The Future of AI in Software Development</title>
        </head>
        <body>
            <div class="container">
                <header>
                    <div class="logo">TechNews</div>
                    <nav>Menu items...</nav>
                </header>

                <main role="main">
                    <article>
                        <h1>The Future of AI in Software Development</h1>

                        <div class="article-body">
                            <p>Artificial Intelligence is transforming how we write code. From intelligent autocomplete to automated code review, AI tools are becoming essential parts of the modern developer's toolkit.</p>

                            <p>This transformation is not without challenges. Developers must learn to work alongside AI systems, understanding both their capabilities and limitations. The tools that emerge from this partnership will shape the future of software engineering.</p>

                            <p>Machine learning models trained on millions of code examples can now suggest entire functions, detect bugs, and even refactor legacy code. These capabilities promise to dramatically increase developer productivity.</p>

                            <h2>Key Trends</h2>

                            <p>Several trends are emerging in AI-assisted development:</p>

                            <ul>
                                <li><strong>Context-aware code completion:</strong> Going beyond simple autocomplete to understand the broader context of what you're building.</li>
                                <li><strong>Automated testing:</strong> AI systems that can generate comprehensive test suites based on your code.</li>
                                <li><strong>Intelligent refactoring:</strong> Tools that can safely modernize legacy codebases.</li>
                            </ul>

                            <p>The integration of AI into development workflows is accelerating. What was cutting-edge research a few years ago is now available in mainstream IDEs and text editors.</p>

                            <p>As these tools mature, they will fundamentally change how we think about software development. The question is not whether AI will transform coding, but how we can best harness its potential while maintaining the craft and creativity that makes great software.</p>
                        </div>
                    </article>
                </main>

                <aside class="related">
                    <h3>Related Articles</h3>
                    <ul>
                        <li><a href="/article1">Article 1</a></li>
                        <li><a href="/article2">Article 2</a></li>
                    </ul>
                </aside>

                <footer>
                    <p>© 2024 TechNews</p>
                </footer>
            </div>
        </body>
        </html>
    "#;

    let result = service
        .extract_article(html, "https://technews.example.com/ai-future")
        .await;

    assert!(result.is_ok());
    let article = result.unwrap();

    // Verify full content extraction
    assert!(
        article.word_count > 50,
        "Should extract substantial content"
    );
    assert!(
        article.text_content.contains("Artificial Intelligence"),
        "Should contain article start"
    );
    assert!(
        article.text_content.contains("craft and creativity"),
        "Should contain article end"
    );
    assert!(
        article
            .text_content
            .contains("Context-aware code completion"),
        "Should contain list items"
    );

    // Verify unwanted content removed
    assert!(
        !article.text_content.contains("Related Articles"),
        "Should not include sidebar"
    );

    println!("\n=== Complex HTML Extraction Test ===");
    println!("Title: {}", article.title);
    println!("Word count: {}", article.word_count);
    println!("✓ Complex article structure handled correctly!");
}

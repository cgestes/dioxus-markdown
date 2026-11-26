import UIKit
import WebKit
import UniformTypeIdentifiers
import os

/// Hosts the browser notebook (mockup/index.html, bundled at build time) in a WKWebView.
///
/// The page is served from `margin://notebook/` rather than a file URL so it has a stable,
/// secure origin: WebCrypto works, and IndexedDB keeps the encrypted notebook, keys and
/// pairing state across launches and app updates.
final class NotebookViewController: UIViewController {
    private static let paper = UIColor(red: 0xfc / 255, green: 0xfb / 255, blue: 0xf8 / 255, alpha: 1)

    let webView: WKWebView
    private var downloads: [WKDownload: URL] = [:]

    /// `configuration` is supplied by WebKit when the page opens a second editor window.
    init(configuration: WKWebViewConfiguration? = nil) {
        let opened = configuration != nil
        let configuration = configuration ?? Self.makeConfiguration()
        webView = WKWebView(frame: .zero, configuration: configuration)
        super.init(nibName: nil, bundle: nil)
        webView.navigationDelegate = self
        webView.uiDelegate = self
        webView.allowsLinkPreview = false
        webView.isOpaque = false
        webView.backgroundColor = Self.paper
        webView.scrollView.backgroundColor = Self.paper
        webView.scrollView.contentInsetAdjustmentBehavior = .never
        #if DEBUG
        webView.isInspectable = true
        #endif
        if !opened { webView.load(URLRequest(url: NotebookSchemeHandler.start)) }
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    override func loadView() { view = webView }

    private static func makeConfiguration() -> WKWebViewConfiguration {
        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = .default()
        configuration.setURLSchemeHandler(NotebookSchemeHandler(), forURLScheme: NotebookSchemeHandler.scheme)
        #if DEBUG
        ConsoleRelay.install(in: configuration.userContentController)
        #endif
        return configuration
    }

    /// Offers a finished download (Export .md, attachments) through the share sheet.
    private func share(_ file: URL) {
        let sheet = UIActivityViewController(activityItems: [file], applicationActivities: nil)
        sheet.popoverPresentationController?.sourceView = view
        sheet.popoverPresentationController?.sourceRect = CGRect(x: view.bounds.midX, y: view.bounds.maxY - 40, width: 0, height: 0)
        present(sheet, animated: true)
    }
}

extension NotebookViewController: WKNavigationDelegate {
    func webView(_ webView: WKWebView, decidePolicyFor action: WKNavigationAction,
                 decisionHandler: @escaping @MainActor (WKNavigationActionPolicy) -> Void) {
        if action.shouldPerformDownload { return decisionHandler(.download) }
        // Links inside notes open in Safari; the notebook itself stays on margin://.
        if let url = action.request.url, ["http", "https", "mailto", "tel"].contains(url.scheme ?? "") {
            UIApplication.shared.open(url)
            return decisionHandler(.cancel)
        }
        decisionHandler(.allow)
    }

    func webView(_ webView: WKWebView, navigationAction: WKNavigationAction, didBecome download: WKDownload) {
        download.delegate = self
    }

    func webView(_ webView: WKWebView, navigationResponse: WKNavigationResponse, didBecome download: WKDownload) {
        download.delegate = self
    }

    func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {
        // iOS reclaims background web processes; reload instead of showing a blank page.
        webView.reload()
    }
}

extension NotebookViewController: WKDownloadDelegate {
    func download(_ download: WKDownload, decideDestinationUsing response: URLResponse, suggestedFilename: String,
                  completionHandler: @escaping @MainActor (URL?) -> Void) {
        let folder = FileManager.default.temporaryDirectory.appending(path: UUID().uuidString, directoryHint: .isDirectory)
        do {
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            let file = folder.appending(path: suggestedFilename)
            downloads[download] = file
            completionHandler(file)
        } catch {
            completionHandler(nil)
        }
    }

    func downloadDidFinish(_ download: WKDownload) {
        if let file = downloads.removeValue(forKey: download) { share(file) }
    }

    func download(_ download: WKDownload, didFailWithError error: Error, resumeData: Data?) {
        downloads[download] = nil
    }
}

extension NotebookViewController: WKUIDelegate {
    /// **Collaborate → Open second editor** opens a sheet sharing this notebook's storage,
    /// so the two-writer demo works on a phone too.
    func webView(_ webView: WKWebView, createWebViewWith configuration: WKWebViewConfiguration,
                 for action: WKNavigationAction, windowFeatures: WKWindowFeatures) -> WKWebView? {
        guard let url = action.request.url else { return nil }
        guard url.scheme == NotebookSchemeHandler.scheme else {
            UIApplication.shared.open(url)
            return nil
        }
        let editor = NotebookViewController(configuration: configuration)
        present(editor, animated: true)
        return editor.webView
    }

    func webViewDidClose(_ webView: WKWebView) {
        if webView !== self.webView { return }
        presentingViewController?.dismiss(animated: true)
    }
}

/// Serves the bundled notebook page under `margin://notebook/`.
final class NotebookSchemeHandler: NSObject, WKURLSchemeHandler {
    static let scheme = "margin"
    static let start = URL(string: "\(scheme)://notebook/index.html")!

    func webView(_ webView: WKWebView, start task: any WKURLSchemeTask) {
        guard let url = task.request.url else { return task.didFailWithError(URLError(.badURL)) }
        // The page is self-contained: fonts, styles, scripts and WebAssembly are inlined.
        guard url.path == "/" || url.path == "/index.html",
              let file = Bundle.main.url(forResource: "index", withExtension: "html"),
              let body = try? Data(contentsOf: file, options: .mappedIfSafe) else {
            task.didReceive(HTTPURLResponse(url: url, statusCode: 404, httpVersion: "HTTP/1.1", headerFields: [:])!)
            task.didReceive(Data())
            return task.didFinish()
        }
        let headers = ["Content-Type": "text/html; charset=utf-8", "Content-Length": String(body.count)]
        task.didReceive(HTTPURLResponse(url: url, statusCode: 200, httpVersion: "HTTP/1.1", headerFields: headers)!)
        task.didReceive(body)
        task.didFinish()
    }

    func webView(_ webView: WKWebView, stop task: any WKURLSchemeTask) {}
}

#if DEBUG
/// Forwards the page's console and uncaught errors to the unified log
/// (Xcode console, or `log stream --predicate 'subsystem == "dev.margin.notebook"'`).
final class ConsoleRelay: NSObject, WKScriptMessageHandler {
    private static let log = Logger(subsystem: Bundle.main.bundleIdentifier ?? "Margin", category: "web")
    private static let script = """
    (() => {
      const text = value => { try { return typeof value === "string" ? value : value instanceof Error ? value.stack || value.message : JSON.stringify(value); } catch { return String(value); } };
      const post = (level, args) => { try { webkit.messageHandlers.console.postMessage(level + " " + args.map(text).join(" ")); } catch {} };
      for (const level of ["log", "info", "warn", "error"]) {
        const original = console[level];
        console[level] = (...args) => { post(level, args); original.apply(console, args); };
      }
      addEventListener("error", event => post("error", [event.message, event.filename + ":" + event.lineno]));
      addEventListener("unhandledrejection", event => post("error", ["unhandled rejection", event.reason]));
    })();
    """

    static func install(in controller: WKUserContentController) {
        controller.add(ConsoleRelay(), name: "console")
        controller.addUserScript(WKUserScript(source: script, injectionTime: .atDocumentStart, forMainFrameOnly: true))
    }

    func userContentController(_ controller: WKUserContentController, didReceive message: WKScriptMessage) {
        Self.log.info("\(String(describing: message.body), privacy: .public)")
    }
}
#endif

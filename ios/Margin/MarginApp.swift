import SwiftUI

@main
struct MarginApp: App {
    var body: some Scene {
        WindowGroup {
            // The page draws under the status bar and home indicator and pads itself with
            // env(safe-area-inset-*), so the web view fills the whole window.
            NotebookView().ignoresSafeArea()
        }
    }
}

struct NotebookView: UIViewControllerRepresentable {
    func makeUIViewController(context: Context) -> NotebookViewController { NotebookViewController() }
    func updateUIViewController(_ controller: NotebookViewController, context: Context) {}
}

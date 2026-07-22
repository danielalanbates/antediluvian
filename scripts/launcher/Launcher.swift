// Antediluvia Launcher — checks batesai.org for updates, installs them, launches the game.
//
// Update flow: GET https://batesai.org/antediluvia/manifest.json
//   { "version": "0.1.0", "url": "<zip of Antediluvia.app>", "sha256": "...",
//     "notes": "changelog text" }
// If manifest version > installed version, download zip → verify sha256 →
// unzip → swap into place. Install location prefers /Applications; falls back
// to ~/Applications when /Applications isn't writable.
import SwiftUI
import CryptoKit

let MANIFEST_URL = "https://batesai.org/antediluvia/manifest.json"
let GAME_NAME = "Antediluvia.app"

struct Manifest: Decodable {
    let version: String
    let url: String
    let sha256: String
    let notes: String?
}

enum LauncherState: Equatable {
    case checking
    case upToDate
    case updateAvailable(String)
    case downloading(Double)
    case installing
    case error(String)
    case notInstalled(String)   // manifest version available, nothing installed
    case offline                // no manifest reachable, but game installed
}

func versionTuple(_ s: String) -> [Int] {
    s.split(separator: ".").map { Int($0) ?? 0 }
}
func isNewer(_ a: String, than b: String) -> Bool {
    let x = versionTuple(a), y = versionTuple(b)
    for i in 0..<max(x.count, y.count) {
        let xi = i < x.count ? x[i] : 0, yi = i < y.count ? y[i] : 0
        if xi != yi { return xi > yi }
    }
    return false
}

final class LauncherModel: ObservableObject {
    @Published var state: LauncherState = .checking
    @Published var installedVersion: String?
    @Published var notes: String = ""
    var manifest: Manifest?

    var gameURL: URL? {
        let candidates = [
            URL(fileURLWithPath: "/Applications/\(GAME_NAME)"),
            FileManager.default.homeDirectoryForCurrentUser
                .appendingPathComponent("Applications/\(GAME_NAME)"),
        ]
        return candidates.first { FileManager.default.fileExists(atPath: $0.path) }
    }

    func readInstalledVersion() -> String? {
        guard let app = gameURL,
              let plist = NSDictionary(contentsOf: app.appendingPathComponent("Contents/Info.plist")),
              let v = plist["CFBundleShortVersionString"] as? String else { return nil }
        return v
    }

    func check() {
        installedVersion = readInstalledVersion()
        state = .checking
        var req = URLRequest(url: URL(string: MANIFEST_URL)!)
        req.cachePolicy = .reloadIgnoringLocalCacheData
        URLSession.shared.dataTask(with: req) { data, _, err in
            DispatchQueue.main.async {
                guard let data = data, err == nil,
                      let m = try? JSONDecoder().decode(Manifest.self, from: data) else {
                    self.state = self.installedVersion != nil
                        ? .offline
                        : .error("Can't reach the update server and no game is installed. Check your internet connection and relaunch.")
                    return
                }
                self.manifest = m
                self.notes = m.notes ?? ""
                if let iv = self.installedVersion {
                    self.state = isNewer(m.version, than: iv) ? .updateAvailable(m.version) : .upToDate
                } else {
                    self.state = .notInstalled(m.version)
                }
            }
        }.resume()
    }

    func downloadAndInstall() {
        guard let m = manifest, let url = URL(string: m.url) else { return }
        state = .downloading(0)
        let task = URLSession.shared.downloadTask(with: url) { tmp, _, err in
            DispatchQueue.main.async { self.state = .installing }
            guard let tmp = tmp, err == nil else {
                DispatchQueue.main.async { self.state = .error("Download failed: \(err?.localizedDescription ?? "unknown error")") }
                return
            }
            do {
                try self.verifyAndSwap(zip: tmp, expected: m.sha256)
                DispatchQueue.main.async {
                    self.installedVersion = self.readInstalledVersion()
                    self.state = .upToDate
                }
            } catch {
                DispatchQueue.main.async { self.state = .error("Install failed: \(error.localizedDescription)") }
            }
        }
        // Poll progress on the main runloop.
        let obs = task.progress.observe(\.fractionCompleted) { p, _ in
            DispatchQueue.main.async {
                if case .downloading = self.state { self.state = .downloading(p.fractionCompleted) }
            }
        }
        objc_setAssociatedObject(task, "obs", obs, .OBJC_ASSOCIATION_RETAIN)
        task.resume()
    }

    private func verifyAndSwap(zip: URL, expected: String) throws {
        let data = try Data(contentsOf: zip)
        let digest = SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
        guard digest == expected.lowercased() else {
            throw NSError(domain: "Antediluvia", code: 1, userInfo: [
                NSLocalizedDescriptionKey: "Downloaded file failed integrity check (sha256 mismatch). Try again later."])
        }
        let fm = FileManager.default
        let work = fm.temporaryDirectory.appendingPathComponent("antediluvia-update-\(UUID().uuidString)")
        try fm.createDirectory(at: work, withIntermediateDirectories: true)
        defer { try? fm.removeItem(at: work) }
        // ditto preserves signatures/permissions when expanding the zip.
        let unzip = Process()
        unzip.executableURL = URL(fileURLWithPath: "/usr/bin/ditto")
        unzip.arguments = ["-x", "-k", zip.path, work.path]
        try unzip.run(); unzip.waitUntilExit()
        guard unzip.terminationStatus == 0 else {
            throw NSError(domain: "Antediluvia", code: 2, userInfo: [NSLocalizedDescriptionKey: "Couldn't expand the update archive."])
        }
        // The zip may contain the .app at its root or one folder down.
        var appSrc = work.appendingPathComponent(GAME_NAME)
        if !fm.fileExists(atPath: appSrc.path) {
            if let found = (try? fm.contentsOfDirectory(at: work, includingPropertiesForKeys: nil))?
                .compactMap({ dir -> URL? in
                    let c = dir.appendingPathComponent(GAME_NAME)
                    return fm.fileExists(atPath: c.path) ? c : (dir.lastPathComponent == GAME_NAME ? dir : nil)
                }).first {
                appSrc = found
            } else {
                throw NSError(domain: "Antediluvia", code: 3, userInfo: [NSLocalizedDescriptionKey: "Update archive didn't contain \(GAME_NAME)."])
            }
        }
        // Prefer replacing the existing install; else /Applications; else ~/Applications.
        let dest: URL
        if let existing = gameURL {
            dest = existing
        } else if fm.isWritableFile(atPath: "/Applications") {
            dest = URL(fileURLWithPath: "/Applications/\(GAME_NAME)")
        } else {
            let userApps = fm.homeDirectoryForCurrentUser.appendingPathComponent("Applications")
            try? fm.createDirectory(at: userApps, withIntermediateDirectories: true)
            dest = userApps.appendingPathComponent(GAME_NAME)
        }
        if fm.fileExists(atPath: dest.path) { try fm.removeItem(at: dest) }
        try fm.copyItem(at: appSrc, to: dest)
    }

    func play() {
        guard let app = gameURL else { return }
        NSWorkspace.shared.openApplication(at: app, configuration: NSWorkspace.OpenConfiguration()) { _, _ in
            DispatchQueue.main.async { NSApp.terminate(nil) }
        }
    }
}

struct LauncherView: View {
    @ObservedObject var model: LauncherModel

    var body: some View {
        VStack(spacing: 18) {
            Spacer().frame(height: 8)
            Text("ANTEDILUVIA")
                .font(.system(size: 42, weight: .heavy, design: .serif))
                .kerning(6)
            Text("The world before the flood")
                .font(.system(size: 13, design: .serif)).italic()
                .foregroundColor(.secondary)

            statusView.frame(maxWidth: .infinity)

            if !model.notes.isEmpty {
                ScrollView {
                    Text(model.notes)
                        .font(.system(size: 12))
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(10)
                }
                .frame(height: 110)
                .background(Color.primary.opacity(0.05))
                .cornerRadius(8)
                .padding(.horizontal, 24)
            }

            buttons
            Spacer()
            Text(footerText).font(.system(size: 10)).foregroundColor(.secondary).padding(.bottom, 10)
        }
        .frame(width: 460, height: 420)
        .onAppear { model.check() }
    }

    var footerText: String {
        var parts: [String] = []
        if let v = model.installedVersion { parts.append("Installed: v\(v)") }
        parts.append("batesai.org/antediluvia")
        return parts.joined(separator: "   ·   ")
    }

    @ViewBuilder var statusView: some View {
        switch model.state {
        case .checking:
            HStack { ProgressView().scaleEffect(0.6); Text("Checking for updates…") }
        case .upToDate:
            Label("You're up to date.", systemImage: "checkmark.circle.fill").foregroundColor(.green)
        case .updateAvailable(let v):
            Label("Update available: v\(v)", systemImage: "arrow.down.circle.fill").foregroundColor(.orange)
        case .notInstalled(let v):
            Label("Ready to install Antediluvia v\(v)", systemImage: "arrow.down.circle.fill")
        case .downloading(let f):
            VStack {
                ProgressView(value: f).padding(.horizontal, 40)
                Text("Downloading… \(Int(f * 100))%").font(.caption)
            }
        case .installing:
            HStack { ProgressView().scaleEffect(0.6); Text("Installing…") }
        case .offline:
            Label("Offline — update check skipped.", systemImage: "wifi.slash").foregroundColor(.secondary)
        case .error(let msg):
            Text(msg).font(.caption).foregroundColor(.red).padding(.horizontal, 24)
                .multilineTextAlignment(.center)
        }
    }

    @ViewBuilder var buttons: some View {
        HStack(spacing: 14) {
            switch model.state {
            case .updateAvailable:
                Button("Update") { model.downloadAndInstall() }.buttonStyle(.borderedProminent).controlSize(.large)
                Button("Play anyway") { model.play() }.controlSize(.large)
            case .notInstalled:
                Button("Install") { model.downloadAndInstall() }.buttonStyle(.borderedProminent).controlSize(.large)
            case .upToDate, .offline:
                Button("Play") { model.play() }.buttonStyle(.borderedProminent).controlSize(.large)
            case .error:
                Button("Retry") { model.check() }.controlSize(.large)
                if model.gameURL != nil { Button("Play") { model.play() }.controlSize(.large) }
            default:
                EmptyView()
            }
        }
    }
}

@main
struct AntediluviaLauncherApp: App {
    @StateObject var model = LauncherModel()
    var body: some Scene {
        WindowGroup { LauncherView(model: model) }
            .windowResizability(.contentSize)
    }
}

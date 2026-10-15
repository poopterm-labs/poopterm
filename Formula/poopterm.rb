class Poopterm < Formula
  desc "Small, poop-colored terminal emulator"
  homepage "https://github.com/poopterm-labs/poopterm"
  url "https://github.com/poopterm-labs/poopterm/archive/b937a79.tar.gz"
  version "0.2.0"
  revision 2
  sha256 "04076ae729bd9f76dc7a7409084f71f7218ea67f02541871eab66be87231f63e"
  license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "build", "--release", "--locked"
    unless OS.mac?
      bin.install "target/release/poopterm"
      return
    end

    app = prefix/"Poopterm.app"
    macos = app/"Contents/MacOS"
    resources = app/"Contents/Resources"
    macos.mkpath
    resources.mkpath
    (macos/"Poopterm").write (buildpath/"target/release/poopterm").binread
    (resources/"Poopterm.icns").write (buildpath/"assets/poopterm.icns").binread
    (app/"Contents/Info.plist").write <<~PLIST
      <?xml version="1.0" encoding="UTF-8"?>
      <!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
      <plist version="1.0">
      <dict>
        <key>CFBundleExecutable</key><string>Poopterm</string>
        <key>CFBundleIdentifier</key><string>labs.poopterm.app</string>
        <key>CFBundleName</key><string>Poopterm</string>
        <key>CFBundleDisplayName</key><string>Poopterm</string>
        <key>CFBundlePackageType</key><string>APPL</string>
        <key>CFBundleIconFile</key><string>Poopterm.icns</string>
        <key>CFBundleVersion</key><string>#{version}</string>
        <key>CFBundleShortVersionString</key><string>#{version}</string>
        <key>LSMinimumSystemVersion</key><string>11.0</string>
        <key>NSHighResolutionCapable</key><true/>
      </dict>
      </plist>
    PLIST
    chmod 0755, macos/"Poopterm"
    bin.install "target/release/poopterm"
  end

  test do
    assert_predicate bin/"poopterm", :executable?
    if OS.mac?
      assert_predicate prefix/"Poopterm.app/Contents/MacOS/Poopterm", :executable?
      assert_predicate prefix/"Poopterm.app/Contents/Resources/Poopterm.icns", :exist?
    end
  end

  def caveats
    return unless OS.mac?

    "The app bundle is installed at #{opt_prefix}/Poopterm.app. Open it with:\n  open #{opt_prefix}/Poopterm.app"
  end
end

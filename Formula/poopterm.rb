class Poopterm < Formula
  desc "Small, poop-colored terminal emulator"
  homepage "https://github.com/poopterm-labs/poopterm"
  url "https://github.com/poopterm-labs/poopterm/archive/09af1ab3be3f74a33adb4389f5451a08ec96bd8b.tar.gz"
  version "0.1.0"
  sha256 "fee81af7c140aa011d06483233aaf59ed8daf42df45f270970fe7ae74bbb5d7a"
  license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "install", "--locked", "--path", ".", "--root", prefix
  end

  test do
    assert_predicate bin/"poopterm", :executable?
  end
end

# Homebrew formula for prr-suggest, a fork of prr that posts code suggestions only.
#
#   brew tap tineoc/prr https://github.com/tineoc/prr
#   brew install --HEAD prr-suggest
class PrrSuggest < Formula
  desc "Mailing list style code reviews for GitHub, suggestions only"
  homepage "https://github.com/tineoc/prr"
  head "https://github.com/tineoc/prr.git", branch: "master"
  license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "build", "--release", "--locked"
    # Installed as prr-suggest so it can coexist with upstream prr
    bin.install "target/release/prr" => "prr-suggest"
  end

  test do
    assert_match "prr", shell_output("#{bin}/prr-suggest --version")
  end
end

#!/usr/bin/env bash
# Builds an UNSIGNED MicNexus.ipa (macOS + Xcode). Sideloadly signs it with
# the user's own Apple ID when installing (docs/IOS.md).
#
#   bash apps/mobile/scripts/build-ipa.sh      -> apps/mobile/ios/build/micnexus-ios.ipa
set -euo pipefail
cd "$(dirname "$0")/../ios"

xcodegen generate
pod install

xcodebuild \
  -workspace MicNexus.xcworkspace \
  -scheme MicNexus \
  -configuration Release \
  -sdk iphoneos \
  -destination 'generic/platform=iOS' \
  -archivePath build/MicNexus.xcarchive \
  archive \
  CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO CODE_SIGN_IDENTITY="" \
  -quiet

rm -rf build/Payload build/micnexus-ios.ipa
mkdir -p build/Payload
cp -R build/MicNexus.xcarchive/Products/Applications/MicNexus.app build/Payload/
test -d build/Payload/MicNexus.app/PlugIns/MicNexusLiveActivity.appex || { echo "Live Activity extension missing" >&2; exit 1; }
test -f build/Payload/MicNexus.app/main.jsbundle || { echo "JS bundle missing" >&2; exit 1; }
(cd build && zip -qry micnexus-ios.ipa Payload)
ls -la build/micnexus-ios.ipa

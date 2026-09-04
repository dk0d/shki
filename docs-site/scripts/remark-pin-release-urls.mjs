// Pin GitHub release URLs to the docs version a page belongs to.
//
// Archived pages live under src/content/docs/<X.Y.Z>/; any
// `releases/latest/download` URL on them (install scripts, download links) is
// rewritten to `releases/download/vX.Y.Z` so versioned docs install the
// release they document. Latest docs are left untouched.
const VERSIONED_PAGE = /[\\/]content[\\/]docs[\\/](\d+\.\d+\.\d+)[\\/]/;
const LATEST_URL = "releases/latest/download";

export default function remarkPinReleaseUrls() {
  return (tree, file) => {
    const version = file.path?.match(VERSIONED_PAGE)?.[1];
    if (!version) return;
    const pinned = `releases/download/v${version}`;
    visit(tree, (node) => {
      if (typeof node.value === "string" && node.value.includes(LATEST_URL)) {
        node.value = node.value.replaceAll(LATEST_URL, pinned);
      }
      if (typeof node.url === "string" && node.url.includes(LATEST_URL)) {
        node.url = node.url.replaceAll(LATEST_URL, pinned);
      }
    });
  };
}

function visit(node, fn) {
  fn(node);
  for (const child of node.children ?? []) visit(child, fn);
}

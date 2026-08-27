// Very small, safe subset of markdown: escapes HTML first, then applies
// *bold*, _italic_, `code`, and turns newlines into <br>. No HTML injection.

function escapeHtml(str) {
  return str
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
}

export function formatMessageText(raw) {
  let text = escapeHtml(raw)
  text = text.replace(/`([^`]+)`/g, '<code>$1</code>')
  text = text.replace(/\*([^*]+)\*/g, '<strong>$1</strong>')
  text = text.replace(/_([^_]+)_/g, '<em>$1</em>')
  text = text.replace(/\n/g, '<br>')
  return text
}

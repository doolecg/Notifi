/** Release notes Markdown as HTML: headings, lists, rules, paragraphs, bold, italic, code and http(s) links.
 *  Everything is escaped first, so the notes can never add their own HTML. */
const escape = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

function inline(s: string) {
  return escape(s)
    .replace(/`([^`]+)`/g, '<code>$1</code>')
    .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
    .replace(/(^|[^*])\*([^*]+)\*/g, '$1<em>$2</em>')
    .replace(/\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)/g, '<a href="$2" target="_blank" rel="noreferrer">$1</a>');
}

export function markdown(text: string): string {
  const out: string[] = [];
  let list = false;
  let para: string[] = [];
  const flush = () => {
    if (para.length) out.push(`<p>${inline(para.join(' '))}</p>`);
    para = [];
    if (list) out.push('</ul>');
    list = false;
  };
  for (const raw of text.replace(/\r\n/g, '\n').split('\n')) {
    const line = raw.trim();
    const heading = /^(#{1,4})\s+(.*)$/.exec(line);
    const item = /^[-*]\s+(.*)$/.exec(line);
    if (!line) {
      flush();
    } else if (/^(-{3,}|\*{3,})$/.test(line)) {
      flush();
      out.push('<hr>');
    } else if (heading) {
      flush();
      out.push(`<h${heading[1].length}>${inline(heading[2])}</h${heading[1].length}>`);
    } else if (item) {
      if (para.length) flush();
      if (!list) out.push('<ul>');
      list = true;
      out.push(`<li>${inline(item[1])}</li>`);
    } else {
      if (list) flush();
      para.push(line);
    }
  }
  flush();
  return out.join('');
}

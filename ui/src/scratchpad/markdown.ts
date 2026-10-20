const escapeHtml = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

function inline(raw: string): string {
  const codes: string[] = [];
  let s = escapeHtml(raw).replace(/`([^`]+)`/g, (_, c) => {
    codes.push(c);
    return `\u0000${codes.length - 1}\u0000`;
  });
  s = s
    .replace(/\[([^\]]+)\]\((https?:\/\/[^\s)]+|mailto:[^\s)]+)\)/g, '<a href="$2" target="_blank" rel="noreferrer noopener">$1</a>')
    .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
    .replace(/(^|[^*])\*([^*\s][^*]*)\*/g, '$1<em>$2</em>');
  return s.replace(/\u0000(\d+)\u0000/g, (_, i) => `<code>${codes[Number(i)]}</code>`);
}

/** Renders a note's Markdown. Task checkboxes carry the source line in `data-line`. */
export function render(text: string): string {
  const lines = text.split('\n');
  const out: string[] = [];
  let list: 'ul' | 'ol' | null = null;
  let fence = false;
  let para: string[] = [];

  const flush = () => {
    if (para.length) out.push(`<p>${para.map(inline).join('<br>')}</p>`);
    para = [];
  };
  const close = () => {
    if (list) out.push(`</${list}>`);
    list = null;
  };

  lines.forEach((line, i) => {
    if (/^```/.test(line)) {
      flush();
      close();
      out.push(fence ? '</code></pre>' : '<pre><code>');
      fence = !fence;
      return;
    }
    if (fence) {
      out.push(escapeHtml(line) + '\n');
      return;
    }
    let m: RegExpMatchArray | null;
    if ((m = line.match(/^\s*[-*+]\s+\[([ xX])\]\s+(.*)$/))) {
      flush();
      if (list !== 'ul') {
        close();
        out.push('<ul class="tasks">');
        list = 'ul';
      }
      const done = m[1] !== ' ';
      out.push(`<li class="task${done ? ' done' : ''}"><input type="checkbox" data-line="${i}"${done ? ' checked' : ''}><span>${inline(m[2])}</span></li>`);
    } else if ((m = line.match(/^\s*[-*+]\s+(.*)$/))) {
      flush();
      if (list !== 'ul') {
        close();
        out.push('<ul>');
        list = 'ul';
      }
      out.push(`<li>${inline(m[1])}</li>`);
    } else if ((m = line.match(/^\s*\d+[.)]\s+(.*)$/))) {
      flush();
      if (list !== 'ol') {
        close();
        out.push('<ol>');
        list = 'ol';
      }
      out.push(`<li>${inline(m[1])}</li>`);
    } else if ((m = line.match(/^(#{1,6})\s+(.*)$/))) {
      flush();
      close();
      out.push(`<h${m[1].length}>${inline(m[2])}</h${m[1].length}>`);
    } else if ((m = line.match(/^>\s?(.*)$/))) {
      flush();
      close();
      out.push(`<blockquote>${inline(m[1])}</blockquote>`);
    } else if (/^\s*([-*_])\1{2,}\s*$/.test(line)) {
      flush();
      close();
      out.push('<hr>');
    } else if (line.trim() === '') {
      flush();
      close();
    } else {
      close();
      para.push(line);
    }
  });
  flush();
  close();
  if (fence) out.push('</code></pre>');
  return out.join('\n');
}

/** Flips the task on `line`; returns the new text. */
export function toggleTask(text: string, line: number): string {
  const lines = text.split('\n');
  lines[line] = lines[line].replace(/\[([ xX])\]/, (_, c) => (c === ' ' ? '[x]' : '[ ]'));
  return lines.join('\n');
}

/** What Shift+Enter should insert after `line` to continue a list, or '' when it is not a list line. */
export function continuation(line: string): string {
  let m: RegExpMatchArray | null;
  if ((m = line.match(/^(\s*[-*+]\s+)\[[ xX]\]\s/))) return `${m[1]}[ ] `;
  if ((m = line.match(/^(\s*[-*+]\s+)/))) return m[1];
  if ((m = line.match(/^(\s*)(\d+)([.)]\s+)/))) return `${m[1]}${Number(m[2]) + 1}${m[3]}`;
  return '';
}

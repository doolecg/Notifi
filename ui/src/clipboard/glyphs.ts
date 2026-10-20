// Line icons for the clipboard and snippet windows, on the same 24 x 24 stroke grid as panel/icons.ts.
export const glyphs = {
  search: 'M10.5 4a6.5 6.5 0 1 1 0 13 6.5 6.5 0 0 1 0-13Z M15.5 15.5 20 20',
  pin: 'M9 4h6l-1 5 3 3v1.5H7V12l3-3-1-5Z M12 13.5V20',
  trash: 'M5 7h14 M10 4h4 M7 7l.8 12.5h8.4L17 7 M10.5 10.5v6 M13.5 10.5v6',
  text: 'M5 6.5h14 M5 10.5h14 M5 14.5h9 M5 18.5h6',
  rich: 'M6 18 10.5 6h1L16 18 M7.6 14h6.8 M17 10h3 M17 14h3',
  image: 'M4 5.5h16v13H4z M4 15.5l4.5-4.5 4 4 2.5-2.5 5 5 M15.5 9.25a.75.75 0 1 1 0 .01Z',
  files: 'M6.5 3.5h7l4 4v13h-11z M13.5 3.5v4h4',
  url: 'M10 14a3.5 3.5 0 0 0 5 0l3-3a3.5 3.5 0 0 0-5-5l-1 1 M14 10a3.5 3.5 0 0 0-5 0l-3 3a3.5 3.5 0 0 0 5 5l1-1',
  color: 'M12 3.5c3.5 4 6 7.2 6 10a6 6 0 0 1-12 0c0-2.8 2.5-6 6-10Z',
  json: 'M9 4.5H8a2 2 0 0 0-2 2v3l-1.5 2.5L6 14.5v3a2 2 0 0 0 2 2h1 M15 4.5h1a2 2 0 0 1 2 2v3l1.5 2.5-1.5 2.5v3a2 2 0 0 1-2 2h-1',
  paste: 'M9 4.5h6v2.5H9z M9 5.5H6.5v15h11v-15H15 M9.5 12h5 M9.5 15.5h3.5',
  copy: 'M8.5 8.5h10v10h-10z M15.5 8.5v-3h-10v10h3',
  stack: 'M12 4 20 8l-8 4-8-4 8-4Z M4 12l8 4 8-4 M4 16l8 4 8-4',
  warning: 'M12 4 21 19.5H3L12 4Z M12 10v4.5 M12 17h.01',
  plus: 'M12 5v14 M5 12h14',
  close: 'M6.5 6.5l11 11 M17.5 6.5l-11 11',
  chevron: 'M9.5 6l6 6-6 6',
  duplicate: 'M8.5 8.5h10v10h-10z M15.5 8.5v-3h-10v10h3 M13.5 11v5 M11 13.5h5',
  snippet: 'M6.5 3.5h8l3 3v14h-11z M9.5 11h5 M9.5 14.5h5 M9.5 18h3',
  calendar: 'M4.5 6h15v13.5h-15z M4.5 10h15 M8.5 3.5V7 M15.5 3.5V7',
  clock: 'M12 3.5a8.5 8.5 0 1 1 0 17 8.5 8.5 0 0 1 0-17Z M12 7.5V12l3 2',
  clipboard: 'M9 4.5h6v2.5H9z M9 5.5H6.5v15h11v-15H15',
  folder: 'M4 7h6l2 2h8v9.5a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 4 18.5Z',
  open: 'M13.5 5.5H18.5V10.5 M18.5 5.5 11 13 M10 6.5H5.5v12h12V14',
} as const;

export type GlyphName = keyof typeof glyphs;

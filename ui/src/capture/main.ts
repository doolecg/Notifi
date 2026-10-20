import { mount } from 'svelte';
import Select from './Select.svelte';
import Preview from './Preview.svelte';
import Editor from './Editor.svelte';
import '../app.css';

const views = { select: Select, preview: Preview, editor: Editor };
const view = views[location.hash.slice(1) as keyof typeof views] ?? Select;

export default mount(view, { target: document.getElementById('app')! });

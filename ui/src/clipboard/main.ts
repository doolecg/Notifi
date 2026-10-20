import { mount } from 'svelte';
import Clipboard from './Clipboard.svelte';
import '../app.css';

export default mount(Clipboard, { target: document.getElementById('app')! });

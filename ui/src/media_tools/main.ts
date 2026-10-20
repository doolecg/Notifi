import { mount } from 'svelte';
import MediaTools from './MediaTools.svelte';
import '../app.css';
import '../lib/fill.css';

export default mount(MediaTools, { target: document.getElementById('app')! });

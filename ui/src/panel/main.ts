import { mount } from 'svelte';
import Panel from './Panel.svelte';
import '../app.css';

export default mount(Panel, { target: document.getElementById('app')! });

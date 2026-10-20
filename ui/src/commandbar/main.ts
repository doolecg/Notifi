import { mount } from 'svelte';
import CommandBar from './CommandBar.svelte';
import '../app.css';

export default mount(CommandBar, { target: document.getElementById('app')! });

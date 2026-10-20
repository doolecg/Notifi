import { mount } from 'svelte';
import Taskbar from './Taskbar.svelte';
import '../app.css';

export default mount(Taskbar, { target: document.getElementById('app')! });

import { mount } from 'svelte';
import Island from './Island.svelte';
import '../app.css';

export default mount(Island, { target: document.getElementById('app')! });

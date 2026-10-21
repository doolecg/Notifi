import { mount } from 'svelte';
import AgentsBar from './AgentsBar.svelte';
import '../app.css';

export default mount(AgentsBar, { target: document.getElementById('app')! });

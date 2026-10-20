import { mount } from 'svelte';
import Snippets from './Snippets.svelte';
import '../app.css';

export default mount(Snippets, { target: document.getElementById('app')! });

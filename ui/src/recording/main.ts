import { mount } from 'svelte';
import Recording from './Recording.svelte';
import '../app.css';

export default mount(Recording, { target: document.getElementById('app')! });

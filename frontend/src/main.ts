import { mount } from 'svelte';
import './app.css';
import Router from './router/Router.svelte';

const target = document.getElementById('app');
if (!target) throw new Error('#app not found');

export default mount(Router, { target });

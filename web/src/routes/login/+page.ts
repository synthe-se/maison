// old bookmarks: signing in happens wherever you are; /login is just the home page
import { redirect } from '@sveltejs/kit';
export const load = () => redirect(307, '/');

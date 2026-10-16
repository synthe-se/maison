// an older link carried its token in its path: it moves to the fragment before anything is
// asked, and the invitation goes on there (no request ever names it in an address)
import { redirect } from '@sveltejs/kit';
export const load = ({ params }: { params: { token: string } }) => redirect(307, `/invite#${encodeURIComponent(params.token)}`);

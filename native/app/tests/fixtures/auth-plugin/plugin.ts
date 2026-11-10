import { definePlugin, ParseError } from 'haiku';
import type { Context, Json } from 'haiku';

const login = (token: string) => ({ credential: { token }, user: { name: 'Fixture User' } });
export default definePlugin({
  id: 'auth.fixture',
  name: '认证离线 Fixture',
  version: '1.0.0',
  auth: {
    qrcode: {
      variants: { expire: '过期测试', wait: '等待确认' },
      async start(variant) {
        return { url: `https://fixture.invalid/login?session=${Date.now()}`,
          state: { variant: variant || 'expire' }, expiresIn: variant === 'wait' ? 30 : 1.5 };
      },
      async poll(_state, ctx) {
        ctx.log.info('QR poll waiting');
        await new Promise<void>((resolve, reject) => {
          const timer = setTimeout(resolve, 10000);
          ctx.signal.addEventListener('abort', () => {
            clearTimeout(timer); reject(new Error('Cancelled QR poll'));
          }, { once: true });
        });
        return { status: 'waiting' as const };
      },
    },
    sms: {
      async send(phone) { return { state: { phone }, cooldown: 2 }; },
      async verify(_state, code) {
        if (code !== '123456') throw new ParseError('Wrong fixture code');
        return login('fake-sms');
      },
    },
    cookie: { async submit(value) { return login(value); } },
    web: {
      url: 'https://fixture.invalid/login',
      async done(page) { return page.cookies.length ? login('fake-web') : null; },
    },
    async user(ctx) { return ctx.credential ? { name: 'Fixture User' } : null; },
    async logout() {},
  },
  track: {
    async get(id, ctx) {
      ctx.log.info('Fixture capability invoked');
      return { id, title: 'Fixture result', artists: [] };
    },
    async streams() { return []; },
  },
});

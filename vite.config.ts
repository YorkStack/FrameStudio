import {defineConfig} from 'vite';
import react from '@vitejs/plugin-react';
// Development only. The browser's loopback Vite process bridges to token-authenticated REST.
export default defineConfig({plugins:[react()],server:{host:'127.0.0.1',port:1420,strictPort:true,watch:{ignored:['**/.artifacts/**','**/src-tauri/**','**/test-results/**','**/dist/**']},proxy:{'/rpc':{target:'http://127.0.0.1:43120',changeOrigin:true,configure(proxy){proxy.on('proxyReq',(request, incoming)=>{const origin=incoming.headers.origin;if(!origin || origin==='http://127.0.0.1:1420')request.removeHeader('origin');});}}}},clearScreen:false});

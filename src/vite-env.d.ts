/// <reference types="vite/client" />

declare module "*.css";
declare module "*.css?inline" {
  const css: string;
  export default css;
}

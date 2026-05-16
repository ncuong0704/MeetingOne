/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: false, // Disabled for BlockNote compatibility
  output: 'export',
  images: {
    unoptimized: true,
  },
  // Add basePath configuration
  basePath: '',
  assetPrefix: '/',

  // Prevent Next.js from bundling BlockNote packages server-side during static generation.
  // BlockNote / ProseMirror access DOM APIs that don't exist in the Node.js build worker.
  experimental: {
    serverExternalPackages: [
      '@blocknote/core',
      '@blocknote/react',
      '@blocknote/shadcn',
      '@blocknote/xl-docx-exporter',
      '@blocknote/xl-pdf-exporter',
    ],
  },

  // Webpack configuration for Tauri
  webpack: (config, { isServer, dev }) => {
    if (!isServer) {
      config.resolve.fallback = {
        ...config.resolve.fallback,
        fs: false,
        path: false,
        os: false,
      };
    }
    return config;
  },
}

module.exports = nextConfig

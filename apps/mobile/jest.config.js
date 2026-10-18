const path = require('path');

module.exports = {
  preset: '@react-native/jest-preset',
  // The first test also pays for the cold Babel transform of React Native.
  testTimeout: 30000,
  // Shared workspace packages (../../packages) resolve their imports from this
  // app's node_modules, like Metro does.
  moduleDirectories: ['node_modules', path.join(__dirname, 'node_modules')],
  transformIgnorePatterns: ['node_modules/(?!(@react-native|react-native|@micnexus)/)'],
};

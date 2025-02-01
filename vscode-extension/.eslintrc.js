module.exports = {
    "env": {
        "browser": true,
        "es2021": true
    },
    "extends": "love",
    "overrides": [
        {
            "env": {
                "node": true
            },
            "files": [
                ".eslintrc.{js,cjs}"
            ],
            "parserOptions": {
                "sourceType": "script"
            }
        }
    ],
    "parserOptions": {
        "ecmaVersion": "latest"
    },
    "rules": {
        "semi": ["error", "always"],
        "@typescript-eslint/semi": ["error", "always"],
        "@typescript-eslint/explicit-function-return-type": "off",
    }
}

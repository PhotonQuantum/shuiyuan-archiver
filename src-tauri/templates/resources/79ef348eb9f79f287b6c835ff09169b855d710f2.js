if ('define' in window) {
define("discourse/theme-47/initializers/theme-field-220-common-html-script-1", ["exports", "discourse/lib/plugin-api"], function (_exports, _pluginApi) {
  "use strict";

  Object.defineProperty(_exports, "__esModule", {
    value: true
  });
  _exports.default = void 0;

  function _applyDecoratedDescriptor(target, property, decorators, descriptor, context) { var desc = {}; Object.keys(descriptor).forEach(function (key) { desc[key] = descriptor[key]; }); desc.enumerable = !!desc.enumerable; desc.configurable = !!desc.configurable; if ('value' in desc || desc.initializer) { desc.writable = true; } desc = decorators.slice().reverse().reduce(function (desc, decorator) { return decorator(target, property, desc) || desc; }, desc); if (context && desc.initializer !== void 0) { desc.value = desc.initializer ? desc.initializer.call(context) : void 0; desc.initializer = undefined; } if (desc.initializer === void 0) { Object.defineProperty(target, property, desc); desc = null; } return desc; }

  var settings = require("discourse/lib/theme-settings-store").getObjectForTheme(47);

  var themePrefix = function themePrefix(key) {
    return "theme_translations.47.".concat(key);
  };

  var _default = {
    name: "theme-field-220-common-html-script-1",
    after: "inject-objects",
    initialize: function initialize() {
      (0, _pluginApi.withPluginApi)("0.8.23", function (api) {
        var _dec, _obj;

        var computed = require("discourse-common/utils/decorators").default;

        api.modifyClass('controller:user', (_dec = computed("viewingSelf", "currentUser.admin"), (_obj = {
          showPrivateMessages: function showPrivateMessages(viewingSelf, isAdmin) {
            return this.siteSettings.enable_personal_messages && viewingSelf;
          }
        }, (_applyDecoratedDescriptor(_obj, "showPrivateMessages", [_dec], Object.getOwnPropertyDescriptor(_obj, "showPrivateMessages"), _obj)), _obj)));
      });
    }
  };
  _exports.default = _default;
});
}

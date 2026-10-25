<?php

namespace ShirabeTest\Exception;

/**
 * An exception class only the plugin declares, so no implementation has a type of its own for it.
 */
class PluginException extends \UnexpectedValueException
{
}
